//! Install + reconcile — the cross-tool agent state layer (contracts.md §C,
//! systemPatterns.md §2–5). This is the differentiator: the AI tools have no
//! install registry, so the app IS the database.
//!
//! - **ledger** (`installs.json`): every install action we performed.
//! - **reconcile**: diff ledger ↔ disk ↔ corpus-index into the 5 states.
//! - **tools / projects**: detected tools and project-scoped install surfaces.
//!
//! Provenance is by hash-match only — we never mutate agent content. An
//! installed file is "ours/current" when its bytes equal a fresh render of its
//! slug for its tool (the deterministic `render/` layer makes that reproducible).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use tauri::{AppHandle, State};

use crate::corpus;
use crate::error::AppError;
use crate::registry;
use crate::render;
use crate::state::AppState;
use crate::types::{
    is_agent_domain, AgentDiff, InstallDomain, InstallRecord, InstallState, InstalledAgent,
    ProjectInfo, Tool, ToolInfo, ToolVersion, UpdateKind,
};
use crate::util::fs::{atomic_write, read_capped};
use crate::util::process::headless_tokio_command;

/// Cap on an installed agent file we read back during reconciliation.
const MAX_INSTALLED_BYTES: u64 = 4 * 1024 * 1024;

// ---------- Ledger persistence ----------

fn ledger_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    let adir = corpus::app_data_dir(app)?;
    Ok(corpus::state_dir(&adir).join("installs.json"))
}

async fn load_ledger(app: &AppHandle) -> Result<Vec<InstallRecord>, AppError> {
    read_ledger_at(&ledger_path(app)?).await
}

/// Read a ledger file, treating ONLY "absent" as "nothing installed".
///
/// This used to map every error to an empty ledger. A permission or I/O error
/// therefore read as "you have nothing installed", and the next save replaced
/// the file with just the new row — silently erasing every agent AND rule row.
/// A corrupt ledger must fail loudly; the user can repair or delete the file.
async fn read_ledger_at(path: &Path) -> Result<Vec<InstallRecord>, AppError> {
    match tokio::fs::read(path).await {
        Ok(bytes) => parse_ledger(&bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(AppError::Io {
            message: format!("read {}: {e}", path.display()),
        }),
    }
}

/// Prove a ledger write can succeed BEFORE the first byte of an install is
/// written.
///
/// Reading catches a corrupt or unreadable ledger; the probe write catches a
/// read-only or full state directory. Without this, a failed ledger save left a
/// file on disk with no row — untracked and not removable through the UI.
pub(crate) async fn ensure_ledger_writable(app: &AppHandle) -> Result<(), AppError> {
    ensure_ledger_writable_at(&ledger_path(app)?).await
}

/// Size of the payload the ledger preflight writes.
///
/// Must stay well above zero: a zero-byte write allocates nothing, succeeds on
/// a full or quota-limited filesystem, and would report a directory as writable
/// when the ledger cannot be saved. A real ledger write is at least a few
/// hundred bytes per row, so this is a realistic lower bound.
const PROBE_PAYLOAD_BYTES: usize = 4096;

/// The payload the preflight actually writes.
///
/// A function, not an inline `vec!`, so a test can assert on the buffer that
/// reaches the filesystem rather than on the constant — a call site that
/// silently wrote an empty buffer would otherwise keep its test green.
fn probe_payload() -> Vec<u8> {
    vec![b' '; PROBE_PAYLOAD_BYTES]
}

/// Path-based core of [`ensure_ledger_writable`], so the preflight is testable
/// without an `AppHandle`.
///
/// The probe writes a real payload, not an empty buffer: a zero-byte write
/// allocates nothing, so it succeeds on a full or quota-limited filesystem and
/// would report "writable" for a directory that cannot take the ledger.
pub(crate) async fn ensure_ledger_writable_at(path: &Path) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| AppError::Internal {
        message: format!("ledger path has no parent: {}", path.display()),
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| AppError::Io {
            message: format!("create state dir {}: {e}", parent.display()),
        })?;
    let _ = read_ledger_at(path).await?;
    // Unique per call: a fixed name made two concurrent installs remove each
    // other's probe and report a bogus "cannot write ledger" failure.
    static PROBE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe = parent.join(format!(".installs-probe-{}-{seq}.tmp", std::process::id()));
    let payload = probe_payload();
    let written = async {
        tokio::fs::write(&probe, &payload)
            .await
            .map_err(|e| AppError::Io {
                message: format!("write {}: {e}", probe.display()),
            })?;
        tokio::fs::remove_file(&probe)
            .await
            .map_err(|e| AppError::Io {
                message: format!("remove {}: {e}", probe.display()),
            })
    }
    .await;
    if written.is_err() {
        let _ = tokio::fs::remove_file(&probe).await;
    }
    written
}

/// Parse an `installs.json` document and backfill `dests` for rows written
/// before multi-file support.
///
/// Split out of [`load_ledger`] so the v0.8.x round-trip test drives the REAL
/// load path instead of a copy of it: a backfill bug would otherwise hide behind
/// a test that re-implemented the same loop.
fn parse_ledger(bytes: &[u8]) -> Result<Vec<InstallRecord>, AppError> {
    let mut recs: Vec<InstallRecord> = serde_json::from_slice(bytes).map_err(|e| AppError::Io {
        message: format!("parse installs.json: {e}"),
    })?;
    for r in &mut recs {
        if r.dests.is_empty() {
            r.dests = vec![r.dest.clone()];
        }
    }
    Ok(recs)
}

async fn save_ledger(app: &AppHandle, records: &[InstallRecord]) -> Result<(), AppError> {
    let path = ledger_path(app)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Io {
                message: format!("create state dir {}: {e}", parent.display()),
            })?;
    }
    let bytes = serde_json::to_vec_pretty(records).map_err(|e| AppError::Io {
        message: format!("serialize installs.json: {e}"),
    })?;
    atomic_write(&path, &bytes).await
}

// ---------- Rule ledger rows (issue #29) ----------
//
// Rules share `installs.json` (spec §5.4) and are told apart by `domain`
// (spec §5.4, ruling C4). Every AGENT-facing reader filters on it: a rule row
// stores a HARNESS id in `tool`, and `antigravity`/`opencode` are byte-equal
// between the two id schemes, so an unfiltered consumer double-counts silently.
//
// The writers below are the write path `rules::commands` (W6) uses. The
// read/pure helpers (`agent_rows`, `rule_rows`, `upsert_rule_row`,
// `remove_rule_row`) are the single implementation those commands are built on.

/// The rows an agent-facing surface may see. Iterates by reference so the
/// reconcile loop does not clone a 100-row ledger.
pub(crate) fn agent_rows(records: &[InstallRecord]) -> impl Iterator<Item = &InstallRecord> {
    records.iter().filter(|r| r.domain == InstallDomain::Agent)
}

/// The rule rows, in ledger order.
pub(crate) fn rule_rows(records: &[InstallRecord]) -> Vec<InstallRecord> {
    records
        .iter()
        .filter(|r| r.domain == InstallDomain::Rule)
        .cloned()
        .collect()
}

/// Identity of a ledger row for a rule install: one row per
/// `(domain, slug, harness, project root)`. `tool` holds the harness id.
fn same_rule_install(a: &InstallRecord, b: &InstallRecord) -> bool {
    a.domain == b.domain && a.slug == b.slug && a.tool == b.tool && a.project_path == b.project_path
}

/// Replace-or-append a rule row — the same retain-then-push `do_install` uses,
/// so re-installing the same `(slug, harness, project)` cannot append a
/// duplicate. Pure so the invariant is testable without an `AppHandle`.
///
/// The domain is forced here, not trusted from the caller: a row reaching the
/// rule writer can never persist as an agent row (which would make the Tools
/// panel count a rule as an agent install).
pub(crate) fn upsert_rule_row(records: &mut Vec<InstallRecord>, record: InstallRecord) {
    let mut record = record;
    record.domain = InstallDomain::Rule;
    records.retain(|r| !same_rule_install(r, &record));
    records.push(record);
}

/// Drop one rule row. Returns whether a row matched. Agent rows can never match
/// (`domain` is part of the key), so this cannot delete an agent install.
pub(crate) fn remove_rule_row(
    records: &mut Vec<InstallRecord>,
    slug: &str,
    harness: &str,
    project_path: Option<&str>,
) -> bool {
    let before = records.len();
    records.retain(|r| {
        !(r.domain == InstallDomain::Rule
            && r.slug == slug
            && r.tool == harness
            && r.project_path.as_deref() == project_path)
    });
    records.len() != before
}

/// Persist a rule install row (append or replace). The ONE write path for rule
/// rows, so the C2 invariant is enforced in a single place.
///
/// Called by `rules::commands::rules_install` **after** the rule file is on disk:
/// a failed write never reaches here, so a ledger row cannot claim a file that
/// is not there.
pub(crate) async fn append_rule_install(
    app: &AppHandle,
    record: InstallRecord,
) -> Result<(), AppError> {
    check_tracked_destinations(&record)?;
    let mut ledger = load_ledger(app).await?;
    upsert_rule_row(&mut ledger, record);
    save_ledger(app, &ledger).await
}

/// Ruling C2: `opencode.json` is a side-effect write, never a tracked
/// destination. `rules_uninstall` deletes every path in `dests`, so a row
/// carrying the plugin config would delete a user's OpenCode config — fail
/// closed instead of persisting a row that makes that possible.
fn check_tracked_destinations(record: &InstallRecord) -> Result<(), AppError> {
    for path in std::iter::once(&record.dest).chain(record.dests.iter()) {
        if crate::rules::opencode::is_plugin_config(Path::new(path)) {
            return Err(AppError::InvalidArgument {
                message: format!(
                    "{path} is an OpenCode config file and is never a tracked install destination"
                ),
            });
        }
    }
    Ok(())
}

/// Remove one rule install row. Returns whether a row was removed; nothing is
/// written when it was not.
pub(crate) async fn remove_rule_install(
    app: &AppHandle,
    slug: &str,
    harness: &str,
    project_path: Option<&str>,
) -> Result<bool, AppError> {
    let mut ledger = load_ledger(app).await?;
    let removed = remove_rule_row(&mut ledger, slug, harness, project_path);
    if removed {
        save_ledger(app, &ledger).await?;
    }
    Ok(removed)
}

/// Every rule row — the source of truth `rules_installs` serves, so install
/// state survives a restart (spec §10.6).
pub(crate) async fn list_rule_installs(app: &AppHandle) -> Result<Vec<InstallRecord>, AppError> {
    Ok(rule_rows(&load_ledger(app).await?))
}

/// `tools_list`'s `installed_count` for one tool. Rule rows are excluded: the
/// `antigravity` and `opencode` harness ids are byte-equal to those agent tool
/// ids, so counting them would inflate the Tools panel (ruling C4).
pub(crate) fn tool_installed_count(records: &[InstallRecord], tool: &str) -> u32 {
    agent_rows(records).filter(|r| r.tool == tool).count() as u32
}

/// `projects_list`'s per-project install counts, agent rows only. A rule
/// installed into a project must not appear as an agent install there (ruling
/// C4, spec §10.8).
pub(crate) fn project_install_counts(records: &[InstallRecord]) -> BTreeMap<String, u32> {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for record in agent_rows(records) {
        if let Some(project) = &record.project_path {
            *counts.entry(project.clone()).or_default() += 1;
        }
    }
    counts
}

fn home() -> Result<PathBuf, AppError> {
    dirs::home_dir().ok_or_else(|| AppError::Io {
        message: "cannot resolve home directory".into(),
    })
}

/// User-scope base directory for a tool's installs **and** detection: the
/// per-tool custom path the user configured (e.g. a WSL home) if any, else the
/// OS home. Project-scope installs ignore this — they resolve against the
/// chosen project root. Because the ledger stores the resolved `dest`, reconcile
/// stays correct with no per-tool logic of its own.
async fn tool_home(state: &AppState, tool: &str) -> Result<PathBuf, AppError> {
    let os_home = home()?;
    let base = state
        .settings
        .read()
        .await
        .effective_settings()
        .map(|s| resolve_tool_base(&s.tool_paths, tool, &os_home))
        .unwrap_or(os_home);
    Ok(base)
}

/// Pure per-tool base resolution: a configured, non-empty custom path wins;
/// otherwise the OS home. Split out from [`tool_home`] so it's unit-testable
/// without standing up an `AppState`.
///
/// `pub(crate)` because the rule dest resolver (`rules::render::harness_base`,
/// ruling A7) reuses this ONE implementation — copying it would let the agent
/// and rule paths drift.
pub(crate) fn resolve_tool_base(
    tool_paths: &std::collections::HashMap<String, String>,
    tool: &str,
    os_home: &Path,
) -> PathBuf {
    tool_paths
        .get(tool)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| os_home.to_path_buf())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// Where overwritten files are preserved before a write replaces them. Lives
/// under app data, NOT inside any tool's agent dir — so the Foreign sweep never
/// mistakes a backup for an installed agent. Every destructive write copies the
/// prior bytes here first, making install/update/restore reversible.
fn backups_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let adir = corpus::app_data_dir(app)?;
    Ok(adir.join("backups"))
}

/// Filesystem-safe variant of an RFC3339 timestamp (no colons).
fn fs_stamp(iso: &str) -> String {
    iso.replace([':', '/'], "-")
}

/// Build the ledger record for a render. Shared by the write path
/// (`write_agent_files`) and the no-write Track path so both agree on what a
/// row looks like.
#[allow(clippy::too_many_arguments)]
fn record_for(
    agent: &crate::types::Agent,
    primary_dest: &Path,
    tool: &str,
    project_root: Option<&Path>,
    rendered_hash: String,
    source_hash: &str,
    body_hash: &str,
    corpus_version: &str,
    installed_at: &str,
    dests: Vec<String>,
    file_hashes: std::collections::BTreeMap<String, String>,
) -> InstallRecord {
    let dests = if dests.is_empty() {
        vec![primary_dest.to_string_lossy().to_string()]
    } else {
        dests
    };
    InstallRecord {
        slug: agent.slug.clone(),
        tool: tool.to_string(),
        scope: render::scope_for(project_root),
        project_path: project_root.map(|p| p.to_string_lossy().to_string()),
        dest: primary_dest.to_string_lossy().to_string(),
        dests,
        file_hashes,
        aggregate_slugs: Vec::new(),
        source_hash: source_hash.to_string(),
        body_hash: body_hash.to_string(),
        rendered_hash,
        installed_at: installed_at.to_string(),
        corpus_version: corpus_version.to_string(),
        domain: InstallDomain::Agent,
        plugin_registered: None,
    }
}

/// Copy `dest`'s current bytes into `backup_dir` before it's overwritten, but
/// only if it exists AND differs from the incoming bytes (no-op writes leave no
/// litter). Backup name keeps the original filename + a timestamp so it's
/// human-recoverable. Best-effort within a still-fallible signature: a failed
/// backup aborts the write (we never overwrite what we couldn't preserve).
///
/// `pub(crate)` because the rule install path (`rules::commands`) reuses it: a
/// rule file overwritten at a user's destination must be recoverable for the
/// same reason an agent file must be, and copying the helper would let the two
/// backup layouts drift.
pub(crate) async fn backup_if_differs(
    dest: &Path,
    new_bytes: &[u8],
    backup_dir: &Path,
    stamp: &str,
) -> Result<(), AppError> {
    let existing = match tokio::fs::read(dest).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(AppError::Io {
                message: format!("read existing file {} before backup: {e}", dest.display()),
            })
        }
    };
    if existing == new_bytes {
        return Ok(()); // identical → not a destructive write
    }
    tokio::fs::create_dir_all(backup_dir)
        .await
        .map_err(|e| AppError::Io {
            message: format!("create backups dir {}: {e}", backup_dir.display()),
        })?;
    let fname = dest
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "agent".into());
    let backup = backup_dir.join(format!("{fname}.{}.bak", fs_stamp(stamp)));
    atomic_write(&backup, &existing).await
}

// ---------- Install / update (shared core) ----------

/// Identity of an AGENT ledger row: one row per `(slug, tool, project root)`.
///
/// `domain` is part of the key on purpose (ruling C4): a rule row whose `tool`
/// is a harness id can be byte-equal to an agent `tool` (`antigravity`,
/// `opencode`), so an agent install/uninstall must never rewrite or delete it.
fn matches_agent_install(
    record: &InstallRecord,
    slug: &str,
    tool: &str,
    project_path: Option<&str>,
) -> bool {
    record.domain == InstallDomain::Agent
        && record.slug == slug
        && record.tool == tool
        && record.project_path.as_deref() == project_path
}

async fn do_install(
    app: &AppHandle,
    state: &AppState,
    slug: String,
    tool: Tool,
    project_path: Option<String>,
) -> Result<InstallRecord, AppError> {
    let corpus = corpus::ensure_corpus(app, state).await?;
    let agent = corpus.get(&slug).ok_or_else(|| AppError::Io {
        message: format!("unknown agent: {slug}"),
    })?;
    let entry = corpus.entry(&slug).ok_or_else(|| AppError::Io {
        message: format!("no corpus-index entry for {slug}"),
    })?;
    let raw = corpus::read_source(app, &agent.category, &slug).await?;

    let home = tool_home(state, &tool).await?;
    let proot = project_path.as_ref().map(PathBuf::from);
    let backups = backups_dir(app)?;
    let mut ledger = load_ledger(app).await?;
    let existing_dest = ledger
        .iter()
        .find(|r| matches_agent_install(r, &slug, &tool, project_path.as_deref()))
        .map(|r| PathBuf::from(&r.dest));
    let record = write_agent_files_to(
        &agent,
        &raw,
        &tool,
        &home,
        proot.as_deref(),
        Some(&backups),
        &entry.source_hash,
        &entry.body_hash,
        &corpus.version(),
        &now_iso(),
        existing_dest.as_deref(),
    )
    .await?;

    ledger.retain(|r| !matches_agent_install(r, &slug, &tool, project_path.as_deref()));
    ledger.push(record.clone());
    save_ledger(app, &ledger).await?;
    Ok(record)
}

/// Track a recognized on-disk agent into the ledger **without writing anything**
/// (contrast `do_install`, which renders + overwrites). We record the canonical
/// render's hash + the current corpus source/body hashes, but leave the user's
/// file exactly as it is. Reconcile then tells the truth: if the on-disk bytes
/// match the canonical render it shows `Current`; if they differ (older catalog
/// version, or hand-edited) it shows `Modified`, and an explicit Update (which
/// backs up first) reconciles it. This is the safe replacement for "Adopt".
async fn do_track(
    app: &AppHandle,
    state: &AppState,
    slug: String,
    tool: Tool,
    project_path: Option<String>,
) -> Result<InstallRecord, AppError> {
    let corpus = corpus::ensure_corpus(app, state).await?;
    let agent = corpus.get(&slug).ok_or_else(|| AppError::Io {
        message: format!("unknown agent: {slug}"),
    })?;
    let entry = corpus.entry(&slug).ok_or_else(|| AppError::Io {
        message: format!("no corpus-index entry for {slug}"),
    })?;
    let raw = corpus::read_source(app, &agent.category, &slug).await?;

    let home = tool_home(state, &tool).await?;
    let proot = project_path.as_ref().map(PathBuf::from);
    let record = track_agent_record(
        &agent,
        &raw,
        &tool,
        &home,
        proot.as_deref(),
        &entry.source_hash,
        &entry.body_hash,
        &corpus.version(),
        &now_iso(),
    )?;

    let mut ledger = load_ledger(app).await?;
    ledger.retain(|r| !matches_agent_install(r, &slug, &tool, project_path.as_deref()));
    ledger.push(record.clone());
    save_ledger(app, &ledger).await?;
    Ok(record)
}

/// Build a ledger record for Track: compute the canonical render's hash and the
/// destination, but write NOTHING. Pure (Tauri-free) so it's unit-testable
/// against a tempdir — and the test can assert no file appears.
#[allow(clippy::too_many_arguments)]
fn track_agent_record(
    agent: &crate::types::Agent,
    raw: &str,
    tool: &str,
    home: &Path,
    project_root: Option<&Path>,
    source_hash: &str,
    body_hash: &str,
    corpus_version: &str,
    installed_at: &str,
) -> Result<InstallRecord, AppError> {
    let (_bytes, rendered_hash) = render::render_with_hash(agent, raw, tool)?;
    let paths = candidate_dests(agent, raw, tool, home, project_root)?;
    let primary = paths.iter().find(|p| p.exists()).unwrap_or(&paths[0]);
    let dests: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    Ok(record_for(
        agent,
        primary,
        tool,
        project_root,
        rendered_hash,
        source_hash,
        body_hash,
        corpus_version,
        installed_at,
        dests,
        std::collections::BTreeMap::new(),
    ))
}

/// Possible physical destinations for one logical install. App-authored files
/// historically used the catalog filename slug; upstream `convert.sh` uses
/// `slugify(name)` for transform tools. Recognize both without changing the
/// catalog's stable identity.
fn candidate_dests(
    agent: &crate::types::Agent,
    raw: &str,
    tool: &str,
    home: &Path,
    project_root: Option<&Path>,
) -> Result<Vec<PathBuf>, AppError> {
    let mut paths = render::dests(tool, &agent.slug, home, project_root)?;
    let conversion_slug = render::output_slug(agent, raw, tool);
    if conversion_slug != agent.slug {
        for path in render::dests(tool, &conversion_slug, home, project_root)? {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

/// True when the tool's per-agent unit is a directory (`{slug}/LEAF`) rather
/// than a bare `{slug}.ext` file — i.e. a dest template has a `/` right after
/// `{slug}`. skill-md tools (Antigravity, Osaurus) are dir-units.
fn tool_is_dir_unit(tool: &str) -> bool {
    let Some(meta) = crate::registry::get(tool) else {
        return false;
    };
    let Some(dest) = meta.dest.as_ref() else {
        return false;
    };
    dest.user.iter().chain(dest.project.iter()).any(|t| {
        t.split_once("{slug}")
            .is_some_and(|(_, after)| after.starts_with('/'))
    })
}

/// Back up divergent files, then remove every existing physical destination.
/// Backup is a separate first pass so a preservation failure cannot occur after
/// an earlier destination has already been deleted.
// The signature mirrors the install path one-for-one (agent, render inputs,
// scope, ledger row, backup dir, timestamp); splitting it into a struct only
// for arity would obscure that mapping.
#[allow(clippy::too_many_arguments)]
async fn remove_agent_files(
    agent: &crate::types::Agent,
    raw: &str,
    tool: &str,
    home: &Path,
    project_root: Option<&Path>,
    ledger_dest: Option<&Path>,
    backup_dir: &Path,
    stamp: &str,
) -> Result<(), AppError> {
    let (canonical, _) = render::render_with_hash(agent, raw, tool)?;
    let mut paths = candidate_dests(agent, raw, tool, home, project_root)?;
    if let Some(path) = ledger_dest {
        let path = path.to_path_buf();
        if !paths.contains(&path) {
            paths.push(path);
        }
    }

    let existing: Vec<PathBuf> = paths.into_iter().filter(|p| p.exists()).collect();
    for (index, path) in existing.iter().enumerate() {
        let backup_stamp = format!("{stamp}-{index}");
        backup_if_differs(path, canonical.as_bytes(), backup_dir, &backup_stamp).await?;
    }
    // Dir-unit tools (skill-md: Antigravity, Osaurus) install each agent as
    // `<slug>/SKILL.md`. Removing only the leaf file orphans the now-empty
    // `<slug>/` dir, which the reconcile scan re-surfaces as an untracked
    // phantom (#60). Prune the agent dir after deleting the file. `remove_dir`
    // is empty-only, so a dir a user dropped their own files into is left be.
    let dir_unit = tool_is_dir_unit(tool);
    for path in existing {
        remove_file_strict(&path).await?;
        if dir_unit {
            if let Some(agent_dir) = path.parent() {
                let _ = tokio::fs::remove_dir(agent_dir).await;
            }
        }
    }
    Ok(())
}

async fn remove_file_strict(path: &Path) -> Result<(), AppError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::Io {
            message: format!("remove agent file {}: {e}", path.display()),
        }),
    }
}

/// Render + write the agent file(s) and build the ledger record. Pure of Tauri
/// (`home`/`project_root` passed explicitly) so the full render→write→record
/// path is unit-testable against a tempdir. Returns the record; caller persists
/// it to the ledger.
///
/// When `backup_dir` is `Some`, any existing dest whose bytes differ from the
/// incoming render is copied there before being overwritten — every destructive
/// write is reversible. `None` skips backups (only for callers that have already
/// guaranteed there's nothing to preserve).
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
async fn write_agent_files(
    agent: &crate::types::Agent,
    raw: &str,
    tool: &str,
    home: &Path,
    project_root: Option<&Path>,
    backup_dir: Option<&Path>,
    source_hash: &str,
    body_hash: &str,
    corpus_version: &str,
    installed_at: &str,
) -> Result<InstallRecord, AppError> {
    write_agent_files_to(
        agent,
        raw,
        tool,
        home,
        project_root,
        backup_dir,
        source_hash,
        body_hash,
        corpus_version,
        installed_at,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn write_agent_files_to(
    agent: &crate::types::Agent,
    raw: &str,
    tool: &str,
    home: &Path,
    project_root: Option<&Path>,
    backup_dir: Option<&Path>,
    source_hash: &str,
    body_hash: &str,
    corpus_version: &str,
    installed_at: &str,
    preferred_dest: Option<&Path>,
) -> Result<InstallRecord, AppError> {
    // Hard gate before any filesystem work: a tool the app does not declare
    // installable must never produce files (issue #49).
    ensure_installable(tool)?;

    // Render all files (single or multi). Fall back to render_with_hash for
    // tools not yet in render_files (keeps backward compat).
    let rendered = match render::render_files(agent, raw, tool) {
        Ok(files) => files,
        Err(_) => {
            let (bytes, _) = render::render_with_hash(agent, raw, tool)?;
            vec![render::RenderedFile {
                relative: String::new(),
                bytes,
            }]
        }
    };
    let mut paths = render::dests(tool, &agent.slug, home, project_root)?;
    let valid = candidate_dests(agent, raw, tool, home, project_root)?;
    let preferred_dest = preferred_dest.filter(|p| valid.iter().any(|v| v == p));
    if let Some(preferred) = preferred_dest {
        if paths.len() == 1 {
            paths[0] = preferred.to_path_buf();
        } else if let Some(index) = paths.iter().position(|p| p == preferred) {
            paths.swap(0, index);
        }
    }

    // Map each dest path to its rendered bytes by filename suffix.
    let mut file_hashes: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    // Composite hash input: length-prefixed (relative, bytes) pairs.
    let mut composite: Vec<u8> = Vec::new();
    for rf in &rendered {
        composite.extend_from_slice(rf.relative.as_bytes());
        composite.push(0);
        composite.extend_from_slice(rf.bytes.as_bytes());
        composite.push(0);
        file_hashes.insert(rf.relative.clone(), render::sha256_hex(rf.bytes.as_bytes()));
    }
    let mut rendered_hash = render::sha256_hex(&composite);
    // Single-file renders must hash the file bytes directly so the existing
    // reconcile contract (`classify(sha256(on_disk), rendered_hash, …)`) keeps
    // working: the composite framing is only meaningful for multi-file tools.
    // Without this, every single-file install would reconcile as `Modified`.
    if rendered.len() == 1 {
        rendered_hash = render::sha256_hex(rendered[0].bytes.as_bytes());
    }

    for (i, dest) in paths.iter().enumerate() {
        // Find matching rendered file by basename.
        let bytes = if rendered.len() == 1 {
            rendered[0].bytes.as_bytes()
        } else {
            let name = dest
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            rendered
                .iter()
                .find(|rf| {
                    rf.relative == name
                        || rf
                            .relative
                            .rsplit('/')
                            .next()
                            .map(|s| s == name)
                            .unwrap_or(false)
                })
                .map(|rf| rf.bytes.as_bytes())
                .unwrap_or(rendered.get(i).map(|r| r.bytes.as_bytes()).unwrap_or(b""))
        };
        if let Some(bdir) = backup_dir {
            backup_if_differs(dest, bytes, bdir, installed_at).await?;
        }
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Io {
                    message: format!("create {}: {e}", parent.display()),
                })?;
        }
        atomic_write(dest, bytes).await?;
    }
    let dests: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    Ok(record_for(
        agent,
        &paths[0],
        tool,
        project_root,
        rendered_hash,
        source_hash,
        body_hash,
        corpus_version,
        installed_at,
        dests,
        file_hashes,
    ))
}

// ---------- Reconciliation core (pure, testable) ----------

/// Classify one ledger row given what's on disk now and the current corpus
/// source hash for that slug. `disk` is `None` when the file is gone, else the
/// SHA-256 of its current bytes. See systemPatterns.md §4.
fn classify(
    disk: Option<&str>,
    rendered_hash: &str,
    record_source: &str,
    corpus_source: Option<&str>,
) -> InstallState {
    match disk {
        None => InstallState::Removed,
        Some(h) if h != rendered_hash => InstallState::Modified,
        Some(_) => match corpus_source {
            Some(cs) if cs == record_source => InstallState::Current,
            Some(_) => InstallState::Outdated,
            // Agent no longer in the corpus (e.g. removed upstream): the file
            // still matches what we wrote, so treat it as current, not stale.
            None => InstallState::Current,
        },
    }
}

/// A `Current` install whose source agent is unchanged can still be stale if
/// the renderer has changed since it was written (a fix to how a tool's file
/// is generated). The file on disk still matches the hash we recorded, so it
/// is not `Modified`; a fresh render of the same source differs, so an update
/// is available. Pure for testing; `fresh_render` is `None` when the agent or
/// its source is unavailable, which never upgrades the state.
fn renderer_outdated(
    state: InstallState,
    recorded_render: &str,
    fresh_render: Option<&str>,
) -> bool {
    state == InstallState::Current && fresh_render.is_some_and(|fresh| fresh != recorded_render)
}

/// True if `file_bytes` are byte-identical to the canonical render of `agent`
/// for `tool`. Pure (no I/O) so it's unit-testable. When they match, the file
/// on disk IS this agent verbatim — there's nothing to "adopt"; reconcile can
/// treat it as `Current` even if we didn't install it.
fn bytes_match_render(
    agent: &crate::types::Agent,
    raw: &str,
    tool: &str,
    file_bytes: &[u8],
) -> bool {
    match render::render_with_hash(agent, raw, tool) {
        Ok((_, expected)) => render::sha256_hex(file_bytes) == expected,
        Err(_) => false,
    }
}

// ---------- Tool detection ----------

fn detect(tool: &str, home: &Path) -> (bool, Option<String>) {
    // Registry-driven: detected if ANY of the tool's `detect.dirs` exists under
    // `home`; the agents dir comes from `detect.agentsDir`. Recognized-only tools
    // (no `detect` block) → (false, None).
    let Some(det) = registry::get(tool).and_then(|m| m.detect.as_ref()) else {
        return (false, None);
    };
    let detected = det.dirs.iter().any(|d| home.join(d).exists());
    let agents_dir = det
        .agents_dir
        .as_ref()
        .map(|sub| home.join(sub).to_string_lossy().to_string());
    (detected, agents_dir)
}

/// The tools Phase 2 can install to — the wired (installable) registry ids, in
/// registry order. Sourced from the embedded JSON so adding a tool is adding a
/// file, not editing this list.
fn supported() -> Vec<&'static str> {
    registry::wired().map(|m| m.id.as_str()).collect()
}

/// Refuse to touch the filesystem for a tool this app does not declare
/// installable.
///
/// The gate lives at the write choke point on purpose: both `do_install` and
/// every test helper reach the disk through `write_agent_files_to`, and a
/// caller-side check is exactly what was missing when Kimi, OpenClaw and
/// Mistral Vibe installed successfully while `supported()` omitted them, so the
/// Tools panel and reconcile never saw the result (issue #49).
fn ensure_installable(tool: &str) -> Result<(), AppError> {
    match registry::get(tool) {
        Some(meta) if meta.installable() => Ok(()),
        Some(meta) => Err(AppError::InvalidArgument {
            message: format!("tool '{}' is not installable in this app", meta.kebab),
        }),
        None => Err(AppError::InvalidArgument {
            message: format!("unknown tool: {tool}"),
        }),
    }
}

// ---------- Tauri commands ----------

/// Install (or re-install) `slug` into `tool`. For project-scoped tools pass
/// the project root in `project_path`. Returns the ledger record.
#[tauri::command]
pub async fn install_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
    tool: Tool,
    project_path: Option<String>,
) -> Result<InstallRecord, AppError> {
    do_install(&app, &state, slug, tool, project_path).await
}

/// Update an install to the current corpus version (re-render + write). The
/// prior file is backed up first (see `do_install`), so an Update applied to a
/// Modified file preserves the user's edits in `backups/` before restoring the
/// canonical render. Separate command from install for intent + UX.
#[tauri::command]
pub async fn update_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
    tool: Tool,
    project_path: Option<String>,
) -> Result<InstallRecord, AppError> {
    do_install(&app, &state, slug, tool, project_path).await
}

/// Track a recognized Foreign install into the ledger **non-destructively** —
/// we record provenance but never write to the user's file. This is the safe
/// replacement for the old "Adopt" (which overwrote the on-disk file). After
/// tracking, reconcile shows `Current` if the file already matches the canonical
/// render, or `Modified` if it differs (then an explicit Update reconciles it).
#[tauri::command]
pub async fn track_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
    tool: Tool,
    project_path: Option<String>,
) -> Result<InstallRecord, AppError> {
    do_track(&app, &state, slug, tool, project_path).await
}

/// Diff what's on disk against the canonical render the app would write — powers
/// "review before Update" without touching any file.
#[tauri::command]
pub async fn agent_diff(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
    tool: Tool,
    project_path: Option<String>,
) -> Result<AgentDiff, AppError> {
    let corpus = corpus::ensure_corpus(&app, &state).await?;
    let agent = corpus.get(&slug).ok_or_else(|| AppError::Io {
        message: format!("unknown agent: {slug}"),
    })?;
    let raw = corpus::read_source(&app, &agent.category, &slug).await?;
    let (proposed, _hash) = render::render_with_hash(&agent, &raw, &tool)?;

    let home = tool_home(&state, &tool).await?;
    let proot = project_path.as_ref().map(PathBuf::from);
    let ledger = load_ledger(&app).await?;
    let ledger_dest = ledger
        .iter()
        .find(|r| matches_agent_install(r, &slug, &tool, project_path.as_deref()))
        .map(|r| PathBuf::from(&r.dest));
    let candidates = candidate_dests(&agent, &raw, &tool, &home, proot.as_deref())?;
    let dest = ledger_dest
        .as_ref()
        .or_else(|| candidates.iter().find(|p| p.exists()))
        .unwrap_or(&candidates[0]);
    let on_disk = match read_capped(dest, MAX_INSTALLED_BYTES).await {
        Ok(b) => Some(String::from_utf8_lossy(&b).into_owned()),
        Err(_) => None,
    };
    let differs = on_disk.as_deref() != Some(proposed.as_str());
    Ok(AgentDiff {
        slug,
        tool,
        project_path,
        dest: dest.to_string_lossy().to_string(),
        on_disk,
        proposed,
        differs,
    })
}

/// Uninstall: remove the written file(s) and the ledger row.
#[tauri::command]
pub async fn uninstall_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
    tool: Tool,
    project_path: Option<String>,
) -> Result<(), AppError> {
    let corpus = corpus::ensure_corpus(&app, &state).await?;
    let agent = corpus.get(&slug).ok_or_else(|| AppError::Io {
        message: format!("unknown agent: {slug}"),
    })?;
    let raw = corpus::read_source(&app, &agent.category, &slug).await?;
    let home = tool_home(&state, &tool).await?;
    let proot = project_path.as_ref().map(PathBuf::from);
    let mut ledger = load_ledger(&app).await?;
    let ledger_dest = ledger
        .iter()
        .find(|r| matches_agent_install(r, &slug, &tool, project_path.as_deref()))
        .map(|r| PathBuf::from(&r.dest));
    remove_agent_files(
        &agent,
        &raw,
        &tool,
        &home,
        proot.as_deref(),
        ledger_dest.as_deref(),
        &backups_dir(&app)?,
        &now_iso(),
    )
    .await?;
    ledger.retain(|r| !matches_agent_install(r, &slug, &tool, project_path.as_deref()));
    save_ledger(&app, &ledger).await?;
    Ok(())
}

/// Forget a project WITHOUT touching the files on disk: drop every ledger row
/// whose `project_path` matches, then save. The agent/skill files this app
/// wrote stay exactly where they are — this only makes the app stop tracking
/// them, so the project leaves the Projects list (the Foreign sweep re-scans
/// only project roots the ledger still references, so dropped rows don't come
/// back). Callers that want the files gone use `uninstall_agent` per row first.
#[tauri::command]
pub async fn project_forget(app: AppHandle, project_path: String) -> Result<(), AppError> {
    let mut ledger = load_ledger(&app).await?;
    prune_project_rows(&mut ledger, &project_path);
    save_ledger(&app, &ledger).await?;
    Ok(())
}

/// Drop every ledger row whose `project_path` matches, keeping all others
/// (other projects AND user-global rows). Pure so it's unit-testable without an
/// AppHandle; the command just wraps it with load/save.
fn prune_project_rows(records: &mut Vec<InstallRecord>, project_path: &str) {
    // Agent rows only (ruling C4). Forgetting a project is an agent action; it
    // must not silently drop the ledger row of a rule installed into that
    // project (rule reconciliation is M2, and losing the row would lose the
    // only record of a file the app wrote).
    records.retain(|r| {
        !(r.domain == InstallDomain::Agent && r.project_path.as_deref() == Some(project_path))
    });
}

/// The reconciled Library view — every ledger row resolved against disk +
/// corpus into one of the 5 states.
#[tauri::command]
pub async fn installs_reconcile(
    app: AppHandle,
    state: State<'_, AppState>,
    project_roots: Vec<String>,
) -> Result<Vec<InstalledAgent>, AppError> {
    let corpus = corpus::ensure_corpus(&app, &state).await?;
    // Ruling C4: rule rows never reach the agent Library. A rule row's `tool` is
    // a harness id, so the classifier would otherwise treat it as an agent
    // install (`antigravity`/`opencode` are byte-equal between the two id
    // schemes). `ledger` keeps every row because the adoption write below must
    // persist the whole file — filtering it would delete the rule rows on disk.
    let mut ledger = load_ledger(&app).await?;
    let agent_ledger: Vec<InstallRecord> = agent_rows(&ledger).cloned().collect();
    let mut out = Vec::with_capacity(agent_ledger.len());
    for r in &agent_ledger {
        let dest = PathBuf::from(&r.dest);
        let disk_hash = if dest.exists() {
            read_capped(&dest, MAX_INSTALLED_BYTES)
                .await
                .ok()
                .map(|b| render::sha256_hex(&b))
        } else {
            None
        };
        let centry = corpus.entry(&r.slug);
        let corpus_source = centry.as_ref().map(|e| e.source_hash.as_str());
        let mut st = classify(
            disk_hash.as_deref(),
            &r.rendered_hash,
            &r.source_hash,
            corpus_source,
        );
        // Re-render an unchanged agent to catch renderer fixes: without this an
        // install written by an older renderer stays `Current` forever.
        let mut renderer_changed = false;
        if st == InstallState::Current {
            if let Some(agent) = corpus.get(&r.slug) {
                let fresh = match corpus::read_source(&app, &agent.category, &r.slug).await {
                    Ok(raw) => render::render_with_hash(&agent, &raw, &r.tool)
                        .ok()
                        .map(|(_, h)| h),
                    Err(_) => None,
                };
                if renderer_outdated(st, &r.rendered_hash, fresh.as_deref()) {
                    st = InstallState::Outdated;
                    renderer_changed = true;
                }
            }
        }
        // Cosmetic vs substantive: only meaningful when Outdated. Body unchanged
        // upstream → the update is metadata-only. A renderer change counts as
        // substantive: it can decide whether the tool loads the agent at all.
        let update_kind = if st == InstallState::Outdated {
            let cur_body = centry.as_ref().map(|e| e.body_hash.as_str());
            Some(
                if !renderer_changed && cur_body == Some(r.body_hash.as_str()) {
                    UpdateKind::Cosmetic
                } else {
                    UpdateKind::Substantive
                },
            )
        } else {
            None
        };
        let name = corpus
            .get(&r.slug)
            .map(|a| a.name)
            .unwrap_or_else(|| r.slug.clone());
        out.push(InstalledAgent {
            slug: r.slug.clone(),
            name,
            tool: r.tool.clone(),
            scope: r.scope,
            project_path: r.project_path.clone(),
            dest: r.dest.clone(),
            state: st,
            update_kind,
            tracked: true,
        });
    }

    // Foreign sweep: files on disk we did NOT install but recognize as corpus
    // agents (slug matches a known agent). A file that is BYTE-IDENTICAL to the
    // canonical render IS that agent, verbatim — installed outside the app (e.g.
    // the CLI install.sh), but in sync. We surface it as `Current` (nothing to
    // decide). Only a recognized-but-DIFFERENT file (older version, or
    // hand-edited) stays `Foreign` and asks for a look. Scans each supported
    // tool's dir(s) — user dirs + every project dir in the ledger.
    let ledger_keys: std::collections::HashSet<(String, Tool, Option<String>)> = agent_ledger
        .iter()
        .map(|r| (r.slug.clone(), r.tool.clone(), r.project_path.clone()))
        .collect();
    // Every project root we know about: ledger dirs UNION the caller's registered
    // project roots. The latter is why a just-added folder (or one whose rows were
    // dropped by "Remove from app only") re-surfaces its on-disk agents instead of
    // staying invisible until something new is installed into it.
    let project_dirs: Vec<PathBuf> = agent_ledger
        .iter()
        .filter_map(|r| r.project_path.clone())
        .chain(project_roots)
        .collect::<std::collections::BTreeSet<String>>()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    // Byte-perfect foreign matches are adopted into the ledger (see below) —
    // collect the new rows here and persist them once after the sweep.
    let mut adopted: Vec<InstallRecord> = Vec::new();
    let mut adopted_seen: std::collections::HashSet<(String, Tool, Option<String>)> =
        std::collections::HashSet::new();
    for tool in supported() {
        // Resolve each tool against its own base (honors a per-tool custom
        // path, e.g. a WSL home), so the sweep looks where the tool lives.
        let home = tool_home(&state, tool).await?;
        // Some tools namespace the output slug (e.g. Osaurus dirs are
        // `agency-<slug>`); strip it before recognizing the agent.
        let prefix = crate::registry::get(tool)
            .and_then(|m| m.slug_prefix.as_deref())
            .unwrap_or("");
        // Dual-scope tools are scanned in BOTH places: the user-global dir (key
        // None) AND every project root the ledger knows about (key Some(path)).
        // Each entry: (scope-key, agents-root, suffix-after-`{slug}`).
        let mut scan_roots: Vec<(Option<String>, PathBuf, String)> = Vec::new();
        if render::supports_user(tool) {
            scan_roots.extend(
                agent_units(tool, &home, None)
                    .into_iter()
                    .map(|(d, s)| (None, d, s)),
            );
        }
        if render::supports_project(tool) {
            scan_roots.extend(project_dirs.iter().flat_map(|p| {
                let key = Some(p.to_string_lossy().to_string());
                agent_units(tool, &home, Some(p))
                    .into_iter()
                    .map(move |(d, s)| (key.clone(), d, s))
            }));
        }
        for (proj, agents_root, suffix) in scan_roots {
            let mut rd = match tokio::fs::read_dir(&agents_root).await {
                Ok(r) => r,
                Err(_) => continue,
            };
            // A leading `/` in the suffix means the per-agent unit is a DIRECTORY
            // (e.g. `{slug}/SKILL.md`); otherwise it's a file (`{slug}.md`).
            let dir_unit = suffix.starts_with('/');
            while let Ok(Some(ent)) = rd.next_entry().await {
                let name = ent.file_name();
                let Some(name) = name.to_str() else { continue };
                // Recover the `{slug}` token + the file that holds the canonical
                // bytes. Dir unit: the entry IS the slug dir, bytes at <dir>/<leaf>.
                // File unit: the entry is `<slug><suffix>`.
                let (token, byte_path) = if dir_unit {
                    (
                        name.to_string(),
                        agents_root.join(name).join(suffix.trim_start_matches('/')),
                    )
                } else if name.ends_with(suffix.as_str()) && name.len() > suffix.len() {
                    (
                        name[..name.len() - suffix.len()].to_string(),
                        agents_root.join(name),
                    )
                } else {
                    continue; // not a unit for this template (stray file/dir)
                };
                let cand = token.strip_prefix(prefix).unwrap_or(&token);
                let Some(agent) = corpus
                    .get(cand)
                    .or_else(|| corpus.get_by_conversion_slug(cand))
                else {
                    continue; // unrecognized → not ours to claim
                };
                let slug = agent.slug.clone();
                if ledger_keys.contains(&(slug.clone(), tool.to_string(), proj.clone())) {
                    continue; // already in the ledger
                }
                // Read the on-disk bytes + canonical source once. A byte-perfect
                // match is unambiguously our render, so ADOPT it into the ledger
                // (tracked) — the app then manages it like any install, whether
                // the CLI or the app wrote it. Only agency-catalog agents ever get
                // here (recognized above), so we never claim unrelated files. A
                // recognized-but-DIVERGENT file stays Foreign + untracked.
                let raw = corpus::read_source(&app, &agent.category, &slug).await.ok();
                let disk = read_capped(&byte_path, MAX_INSTALLED_BYTES).await.ok();
                let canonical = matches!(
                    (raw.as_deref(), disk.as_deref()),
                    (Some(rw), Some(db)) if bytes_match_render(&agent, rw, tool, db)
                );
                let mut tracked = false;
                let state = if canonical {
                    if let (Some(rw), Some(entry)) = (raw.as_deref(), corpus.entry(&slug)) {
                        let key = (slug.clone(), tool.to_string(), proj.clone());
                        if !adopted_seen.contains(&key) {
                            if let Ok(rec) = track_agent_record(
                                &agent,
                                rw,
                                tool,
                                &home,
                                proj.as_deref().map(std::path::Path::new),
                                &entry.source_hash,
                                &entry.body_hash,
                                &corpus.version(),
                                &now_iso(),
                            ) {
                                adopted.push(rec);
                                adopted_seen.insert(key);
                            }
                        }
                        tracked = true;
                    }
                    InstallState::Current
                } else {
                    InstallState::Foreign
                };
                out.push(InstalledAgent {
                    slug,
                    name: agent.name.clone(),
                    tool: tool.to_string(),
                    scope: render::scope_for(proj.as_deref().map(std::path::Path::new)),
                    project_path: proj.clone(),
                    dest: byte_path.to_string_lossy().to_string(),
                    state,
                    update_kind: None,
                    tracked,
                });
            }
        }
    }

    // Persist the byte-perfect adoptions in one write. Idempotent: next reconcile
    // finds them in the ledger (skipped by the sweep), so steady state is no write.
    if !adopted.is_empty() {
        ledger.extend(adopted);
        save_ledger(&app, &ledger).await?;
    }

    // Collapse to one row per LOGICAL install (slug, tool, project). Copilot
    // dual-writes to ~/.github and ~/.copilot, so the Foreign sweep finds the
    // same agent twice; other tools could too. One logical install = one row
    // (its Track/Update/Remove already cover every physical dest).
    let mut seen = std::collections::HashSet::new();
    out.retain(|a| seen.insert((a.slug.clone(), a.tool.clone(), a.project_path.clone())));

    Ok(out)
}

/// For the Foreign sweep: each scannable agents-root for a tool, paired with the
/// path suffix that follows `{slug}` in the dest template. The suffix tells the
/// sweep whether a per-agent UNIT is a file (e.g. `.md`) or a directory (e.g.
/// `/SKILL.md`), and where the canonical bytes live inside a dir unit.
///
/// Splitting the TEMPLATE (not a `{slug}`-substituted path) is what makes
/// dir-structured tools work: Osaurus's `.osaurus/skills/{slug}/SKILL.md` scans
/// `.osaurus/skills` instead of a bogus `.osaurus/skills/_probe`.
fn agent_units(tool: &str, home: &Path, project_root: Option<&Path>) -> Vec<(PathBuf, String)> {
    let Some(meta) = crate::registry::get(tool) else {
        return Vec::new();
    };
    let Some(dest) = meta.dest.as_ref() else {
        return Vec::new();
    };
    let (templates, root): (&[String], &Path) = match project_root {
        Some(p) => (&dest.project, p),
        None => (&dest.user, home),
    };
    templates
        .iter()
        .filter_map(|t| {
            t.split_once("{slug}")
                .map(|(before, after)| (root.join(before), after.to_string()))
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// All install records that match a given agent (for the persona detail panel).
///
/// Agent rows only (ruling C4): a rule row can share the slug, and rules are not
/// agents — the persona panel must not claim one as an install of the agent.
#[tauri::command]
pub async fn installs_for_agent(
    app: AppHandle,
    slug: String,
) -> Result<Vec<InstallRecord>, AppError> {
    let ledger = load_ledger(&app).await?;
    Ok(agent_installs_for(&ledger, &slug))
}

/// The rows a persona's detail panel may show. Pure so the C4 filter is tested
/// without an `AppHandle`.
fn agent_installs_for(records: &[InstallRecord], slug: &str) -> Vec<InstallRecord> {
    agent_rows(records)
        .filter(|r| r.slug == slug)
        .cloned()
        .collect()
}

/// Detected AI tools + their deployment surface and installed counts.
#[tauri::command]
pub async fn tools_list(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ToolInfo>, AppError> {
    let ledger = load_ledger(&app).await?;
    let os_home = home()?;
    let supported = supported();
    let mut out = Vec::with_capacity(supported.len());
    for tool in supported {
        let installed_count = tool_installed_count(&ledger, tool);
        // Resolve against the per-tool base so detection + user_dest reflect a
        // custom path (e.g. a WSL home). custom_path exposes the override to the
        // UI (None when it equals the OS home).
        let home = tool_home(&state, tool).await?;
        let custom_path = (home != os_home).then(|| home.to_string_lossy().to_string());
        let (detected, user_dest) = detect(tool, &home);
        out.push(ToolInfo {
            tool: tool.to_string(),
            label: render::label(tool),
            detected,
            // Primary/display scope: dual-scope tools read "user" (global-first);
            // Cursor is the project-only exception. Per-install scope is derived
            // from the chosen project root, not this field.
            scope: if render::supports_user(tool) {
                crate::types::Scope::User
            } else {
                crate::types::Scope::Project
            },
            user_dest,
            installed_count,
            custom_path,
        });
    }
    Ok(out)
}

/// Open a path in the OS file manager (Finder / Explorer / xdg-open).
/// Best-effort: returns an error the UI can toast if the path is missing or no
/// opener is available. Used by the Tools panel's "Reveal" affordance.
#[tauri::command]
pub async fn reveal_path(path: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        let program = "open";
        #[cfg(target_os = "windows")]
        let program = "explorer";
        #[cfg(all(unix, not(target_os = "macos")))]
        let program = "xdg-open";
        let mut c = std::process::Command::new(program);
        crate::util::proc::sanitize(&mut c);
        c.arg(&path).status().map(|_| ()).map_err(|e| AppError::Io {
            message: format!("could not open {path}: {e}"),
        })
    })
    .await
    .map_err(|e| AppError::Io {
        message: e.to_string(),
    })?
}

/// The `<bin> --version`-style probe command for a tool, or `None` when we don't
/// know one. Best-effort and uneven by nature — GUI tools may not ship a CLI.
fn version_cmd(tool: &str) -> Option<(&'static str, Vec<&'static str>)> {
    // The registry is cached for the process lifetime (`OnceLock`), so its
    // `&str`s are effectively `'static` — fine to hand to the version probe.
    let v = registry::get(tool)?.version.as_ref()?;
    Some((v.bin.as_str(), v.args.iter().map(String::as_str).collect()))
}

/// First non-empty trimmed line of version output, capped to a sane length.
fn first_version_line(s: &str) -> Option<String> {
    s.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| {
        let capped: String = l.chars().take(48).collect();
        capped
    })
}

async fn probe_version(tool: &str) -> Option<String> {
    let (bin, args) = version_cmd(tool)?;
    let mut command = headless_tokio_command(bin);
    crate::util::proc::sanitize_tokio(&mut command);
    command.args(args);
    let fut = command.output();
    match tokio::time::timeout(std::time::Duration::from_secs(3), fut).await {
        Ok(Ok(o)) if o.status.success() => first_version_line(&String::from_utf8_lossy(&o.stdout))
            .or_else(|| first_version_line(&String::from_utf8_lossy(&o.stderr))),
        _ => None,
    }
}

/// Best-effort version probe across all supported tools, run concurrently with a
/// per-tool timeout. A tool whose binary isn't on PATH (or that has no known
/// version command) comes back as `version: None` — the UI just omits it.
#[tauri::command]
pub async fn tool_versions() -> Result<Vec<ToolVersion>, AppError> {
    let supported = supported();
    let mut handles = Vec::with_capacity(supported.len());
    for tool in supported {
        handles.push(tokio::spawn(async move {
            ToolVersion {
                tool: tool.to_string(),
                version: probe_version(tool).await,
            }
        }));
    }
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(v) = h.await {
            out.push(v);
        }
    }
    Ok(out)
}

/// Project directories we've installed project-scoped agents into.
///
/// Agent rows only (ruling C4): a rule installed into a project must not read as
/// an agent install there, which is exactly what spec §10.8 forbids.
#[tauri::command]
pub async fn projects_list(app: AppHandle) -> Result<Vec<ProjectInfo>, AppError> {
    let ledger = load_ledger(&app).await?;
    let counts = project_install_counts(&ledger);
    Ok(counts
        .into_iter()
        .map(|(path, installed_count)| {
            let label = Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            ProjectInfo {
                path,
                label,
                installed_count,
            }
        })
        .collect())
}

// ---------- Loadouts (Agentfile) ----------

/// Portable manifest of an install set — "set up a new Mac in one click".
/// JSON so it's diffable + shareable; `tool` uses the camelCase wire value.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Agentfile {
    /// Format version.
    agentfile: u32,
    installs: Vec<LoadoutEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadoutEntry {
    slug: String,
    tool: Tool,
    #[serde(default)]
    project_path: Option<String>,
    /// Domain discriminator, mirroring the ledger row (ruling C4). Skipped for
    /// agent entries, so an Agentfile exported by this build is byte-identical
    /// to v0.8.x output. A `rule` entry is never imported as an agent: its
    /// `tool` is a harness id, so `do_install` would fail and the entry would
    /// silently restore nothing.
    #[serde(default, skip_serializing_if = "is_agent_domain")]
    domain: InstallDomain,
}

impl From<&InstallRecord> for LoadoutEntry {
    fn from(record: &InstallRecord) -> Self {
        Self {
            slug: record.slug.clone(),
            tool: record.tool.clone(),
            project_path: record.project_path.clone(),
            domain: record.domain,
        }
    }
}

/// The Agentfile for a ledger — agent rows only (ruling C4). Pure so the
/// export surface is testable without an `AppHandle` or a filesystem.
fn agentfile_from(records: &[InstallRecord]) -> Agentfile {
    Agentfile {
        agentfile: 1,
        installs: agent_rows(records).map(LoadoutEntry::from).collect(),
    }
}

/// Export the current ledger as an Agentfile written to `path`. Returns count.
#[tauri::command]
pub async fn loadout_export(app: AppHandle, path: String) -> Result<u32, AppError> {
    let ledger = load_ledger(&app).await?;
    let af = agentfile_from(&ledger);
    let n = af.installs.len() as u32;
    let bytes = serde_json::to_vec_pretty(&af).map_err(|e| AppError::Io {
        message: format!("serialize Agentfile: {e}"),
    })?;
    atomic_write(Path::new(&path), &bytes).await?;
    Ok(n)
}

/// Import an Agentfile from `path`, installing every entry. Returns the records
/// that installed successfully (entries that fail — e.g. a project tool whose
/// path no longer exists — are skipped, not fatal).
#[tauri::command]
pub async fn loadout_import(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<InstallRecord>, AppError> {
    let bytes = read_capped(Path::new(&path), MAX_INSTALLED_BYTES).await?;
    let af: Agentfile = serde_json::from_slice(&bytes).map_err(|e| AppError::Io {
        message: format!("parse Agentfile: {e}"),
    })?;
    let entries = agent_loadout_entries(af);
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        if let Ok(rec) = do_install(&app, &state, e.slug, e.tool, e.project_path).await {
            out.push(rec);
        }
    }
    Ok(out)
}

/// The Agentfile entries an import may restore.
///
/// Agent entries only (ruling C4): a rule entry carries a harness id in `tool`,
/// which is not an agent tool — importing it would fail inside `do_install` and
/// be skipped anyway, but skipping it here keeps the loader's contract explicit
/// instead of relying on a swallowed error. Pure so the surface is testable
/// without an `AppHandle`.
fn agent_loadout_entries(af: Agentfile) -> Vec<LoadoutEntry> {
    af.installs
        .into_iter()
        .filter(|e| e.domain == InstallDomain::Agent)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_ledger_read_error_is_not_an_empty_ledger() {
        // A directory at the ledger path makes `read` fail with something other
        // than NotFound. That MUST propagate: reading it as "nothing installed"
        // let the next save erase every agent and rule row.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("installs.json");
        std::fs::create_dir_all(&path).expect("occupy the ledger path");

        let error = read_ledger_at(&path)
            .await
            .expect_err("a read error must not become an empty ledger");
        assert!(
            error.to_string().contains("read "),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn a_missing_ledger_is_an_empty_ledger() {
        let dir = tempfile::tempdir().expect("tempdir");
        let records = read_ledger_at(&dir.path().join("installs.json"))
            .await
            .expect("a missing ledger is normal");
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn the_preflight_does_not_create_the_ledger() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("state").join("installs.json");
        ensure_ledger_writable_at(&path)
            .await
            .expect("a fresh install must pass the preflight");
        assert!(!path.exists(), "the preflight must not create the ledger");
        let leftovers: Vec<String> = std::fs::read_dir(dir.path().join("state"))
            .expect("list state dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".installs-probe"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "the probe file must be removed; found {leftovers:?}"
        );
    }

    #[test]
    fn the_preflight_payload_is_not_empty() {
        // Pins the fix for F3: an empty probe passes on a full filesystem.
        // `black_box` keeps this a runtime assertion instead of a constant one
        // (clippy denies those, and rightly so).
        let payload = std::hint::black_box(probe_payload());
        assert!(
            payload.len() >= 4096,
            "the preflight must write a realistic payload, not an empty buffer"
        );
    }

    #[tokio::test]
    async fn the_preflight_rejects_an_unreadable_ledger() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("installs.json");
        std::fs::create_dir_all(&path).expect("occupy the ledger path");
        ensure_ledger_writable_at(&path)
            .await
            .expect_err("an unreadable ledger must fail before any write");
    }

    #[tokio::test]
    async fn the_preflight_rejects_an_unwritable_state_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A regular file where the state directory should be: `create_dir_all`
        // cannot succeed, so the preflight fails before any rule is written.
        let state = dir.path().join("state");
        std::fs::write(&state, b"not a directory").expect("occupy the state dir");
        ensure_ledger_writable_at(&state.join("installs.json"))
            .await
            .expect_err("an unwritable state dir must fail before any write");
    }

    #[test]
    fn agentfile_roundtrips() {
        let af = Agentfile {
            agentfile: 1,
            installs: vec![
                LoadoutEntry {
                    slug: "a".into(),
                    tool: "claudeCode".to_string(),
                    project_path: None,
                    domain: InstallDomain::Agent,
                },
                LoadoutEntry {
                    slug: "b".into(),
                    tool: "cursor".to_string(),
                    project_path: Some("/proj".into()),
                    domain: InstallDomain::Agent,
                },
            ],
        };
        let bytes = serde_json::to_vec(&af).unwrap();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("\"claudeCode\"") && s.contains("\"projectPath\":\"/proj\""));
        assert!(
            !s.contains("\"domain\""),
            "an agent entry carries no domain key: {s}"
        );
        let back: Agentfile = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.installs.len(), 2);
        assert_eq!(back.installs[1].tool, "cursor");
    }

    /// A minimal ledger row for the prune test.
    fn row(slug: &str, tool: &str, project: Option<&str>) -> InstallRecord {
        InstallRecord {
            slug: slug.into(),
            tool: tool.to_string(),
            scope: render::scope_for(project.map(Path::new)),
            project_path: project.map(String::from),
            dest: format!("/dest/{slug}"),
            dests: Vec::new(),
            file_hashes: std::collections::BTreeMap::new(),
            aggregate_slugs: Vec::new(),
            source_hash: String::new(),
            body_hash: String::new(),
            rendered_hash: String::new(),
            installed_at: String::new(),
            corpus_version: String::new(),
            domain: InstallDomain::Agent,
            plugin_registered: None,
        }
    }

    #[test]
    fn prune_project_rows_drops_only_that_project() {
        let mut ledger = vec![
            row("a", "claudeCode", Some("/p1")),
            row("b", "cursor", Some("/p1")),
            row("c", "claudeCode", Some("/p2")),
            row("d", "claudeCode", None), // user-global
        ];
        prune_project_rows(&mut ledger, "/p1");
        // Both /p1 rows gone; the other project + the global row survive.
        assert_eq!(ledger.len(), 2);
        assert!(ledger
            .iter()
            .all(|r| r.project_path.as_deref() != Some("/p1")));
        assert!(ledger
            .iter()
            .any(|r| r.slug == "c" && r.project_path.as_deref() == Some("/p2")));
        assert!(ledger
            .iter()
            .any(|r| r.slug == "d" && r.project_path.is_none()));

        // Forgetting an unknown project changes nothing.
        prune_project_rows(&mut ledger, "/nope");
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn classify_states() {
        // file gone
        assert_eq!(classify(None, "r", "s1", Some("s1")), InstallState::Removed);
        // bytes differ from what we wrote → user-edited
        assert_eq!(
            classify(Some("x"), "r", "s1", Some("s1")),
            InstallState::Modified
        );
        // matches our render, corpus unchanged → current
        assert_eq!(
            classify(Some("r"), "r", "s1", Some("s1")),
            InstallState::Current
        );
        // matches our render, corpus advanced → outdated
        assert_eq!(
            classify(Some("r"), "r", "s1", Some("s2")),
            InstallState::Outdated
        );
        // agent gone from corpus but file intact → current
        assert_eq!(classify(Some("r"), "r", "s1", None), InstallState::Current);
    }

    #[test]
    fn renderer_change_makes_a_current_install_outdated() {
        // Same recorded render as a fresh render: still current.
        assert!(!renderer_outdated(InstallState::Current, "r1", Some("r1")));
        // The renderer now writes different bytes for the same source: update.
        assert!(renderer_outdated(InstallState::Current, "r1", Some("r2")));
        // Unknown fresh render (agent or source unavailable): never upgrade.
        assert!(!renderer_outdated(InstallState::Current, "r1", None));
        // Only Current rows are reconsidered; edits and removals win.
        assert!(!renderer_outdated(InstallState::Modified, "r1", Some("r2")));
        assert!(!renderer_outdated(InstallState::Removed, "r1", Some("r2")));
    }

    #[test]
    fn resolve_tool_base_prefers_nonempty_override() {
        use std::collections::HashMap;
        let os = Path::new("/Users/me");
        let mut tp: HashMap<String, String> = HashMap::new();
        // No entry → OS home.
        assert_eq!(
            resolve_tool_base(&tp, "claudeCode", os),
            PathBuf::from("/Users/me")
        );
        // Empty entry is treated as unset → OS home.
        tp.insert("claudeCode".into(), String::new());
        assert_eq!(
            resolve_tool_base(&tp, "claudeCode", os),
            PathBuf::from("/Users/me")
        );
        // Non-empty override wins, and ONLY for that tool.
        tp.insert("claudeCode".into(), "/wsl/home/me".into());
        assert_eq!(
            resolve_tool_base(&tp, "claudeCode", os),
            PathBuf::from("/wsl/home/me")
        );
        assert_eq!(
            resolve_tool_base(&tp, "codex", os),
            PathBuf::from("/Users/me")
        );
    }

    #[test]
    fn agent_units_handles_file_and_dir_tools() {
        let home = std::path::Path::new("/home/u");
        // File-per-agent (Claude): root = ~/.claude/agents, suffix = ".md".
        let claude = agent_units("claudeCode", home, None);
        assert!(
            claude
                .iter()
                .any(|(d, s)| d.ends_with(".claude/agents") && s == ".md"),
            "claude: {claude:?}"
        );
        // Dir-per-agent (Osaurus): the bug was scanning `.osaurus/skills/_probe`.
        // It must scan `.osaurus/skills` with a `/SKILL.md` leaf.
        let osa = agent_units("osaurus", home, None);
        assert_eq!(osa.len(), 1, "osaurus: {osa:?}");
        assert!(
            osa[0].0.ends_with(".osaurus/skills"),
            "osaurus dir: {:?}",
            osa[0].0
        );
        assert_eq!(osa[0].1, "/SKILL.md");
    }

    /// ZCode reads user subagents from ~/.zcode/agents (its Subagents docs);
    /// app v0.3.1 wrote them to ~/.config/zcode/agents. An Update for such an
    /// install must land in the folder ZCode reads, not the recorded old one.
    #[tokio::test]
    async fn update_moves_a_stale_zcode_install_to_the_path_zcode_reads() {
        let home = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\ndescription: Builds UIs.\n---\nBody\n";
        let old = home
            .path()
            .join(".config/zcode/agents/frontend-developer.md");
        let record = write_agent_files_to(
            &agent,
            raw,
            "zcode",
            home.path(),
            None,
            Some(backups.path()),
            "src-1",
            "body-1",
            "v1",
            "2026-10-01T00:00:00Z",
            Some(&old),
        )
        .await
        .unwrap();
        let expected = home.path().join(".zcode/agents/frontend-developer.md");
        assert_eq!(record.dest, expected.to_string_lossy());
        assert!(expected.exists(), "written where ZCode reads subagents");
        assert!(!old.exists(), "the stale recorded path is not recreated");
    }

    fn sample_agent() -> crate::types::Agent {
        crate::types::Agent {
            slug: "frontend-developer".into(),
            name: "Frontend Developer".into(),
            description: "Builds UIs.".into(),
            category: "engineering".into(),
            emoji: None,
            color: Some("blue".into()),
            vibe: None,
            body: "You are a frontend dev.\n".into(),
        }
    }

    /// Full render → write-to-disk → reconcile loop against a tempdir "home".
    #[tokio::test]
    async fn install_writes_then_reconciles_through_states() {
        let home = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\n---\nORIGINAL\n";

        // Codex (user-scoped, TOML transform).
        let rec = write_agent_files(
            &agent,
            raw,
            "codex",
            home.path(),
            None,
            None,
            "src-1",
            "body-1",
            "v1",
            "2026-06-05T00:00:00Z",
        )
        .await
        .unwrap();

        let path = home
            .path()
            .join(".codex")
            .join("agents")
            .join("frontend-developer.toml");
        assert!(path.exists(), "install wrote the file");
        let on_disk = std::fs::read(&path).unwrap();
        let disk_hash = render::sha256_hex(&on_disk);
        assert_eq!(
            disk_hash, rec.rendered_hash,
            "on-disk bytes match recorded render"
        );

        // Reconcile classifications off the real bytes:
        assert_eq!(
            classify(
                Some(&disk_hash),
                &rec.rendered_hash,
                &rec.source_hash,
                Some("src-1")
            ),
            InstallState::Current
        );
        assert_eq!(
            classify(
                Some(&disk_hash),
                &rec.rendered_hash,
                &rec.source_hash,
                Some("src-2")
            ),
            InstallState::Outdated
        );
        assert_eq!(
            classify(
                Some("useredited"),
                &rec.rendered_hash,
                &rec.source_hash,
                Some("src-1")
            ),
            InstallState::Modified
        );
        // delete → Removed
        std::fs::remove_file(&path).unwrap();
        let gone = if path.exists() {
            Some(disk_hash.as_str())
        } else {
            None
        };
        assert_eq!(
            classify(gone, &rec.rendered_hash, &rec.source_hash, Some("src-1")),
            InstallState::Removed
        );
    }

    #[tokio::test]
    async fn claude_code_writes_raw_verbatim() {
        let home = tempfile::tempdir().unwrap();
        let raw = "---\nname: Frontend Developer\ncolor: blue\n---\nVERBATIM BODY\n";
        write_agent_files(
            &sample_agent(),
            raw,
            "claudeCode",
            home.path(),
            None,
            None,
            "s",
            "b",
            "v",
            "t",
        )
        .await
        .unwrap();
        let got = std::fs::read_to_string(home.path().join(".claude/agents/frontend-developer.md"))
            .unwrap();
        assert_eq!(got, raw, "identity tool ships the source unchanged");
    }

    #[tokio::test]
    async fn project_tool_writes_into_project_root() {
        let home = tempfile::tempdir().unwrap();
        let proj = tempfile::tempdir().unwrap();
        let rec = write_agent_files(
            &sample_agent(),
            "raw",
            "cursor",
            home.path(),
            Some(proj.path()),
            None,
            "s",
            "b",
            "v",
            "t",
        )
        .await
        .unwrap();
        assert!(proj
            .path()
            .join(".cursor/rules/frontend-developer.mdc")
            .exists());
        assert_eq!(
            rec.project_path.as_deref(),
            Some(proj.path().to_string_lossy().as_ref())
        );
        assert_eq!(rec.scope, crate::types::Scope::Project);
    }

    /// A file byte-identical to the canonical render is recognized as in-sync
    /// (so the Foreign sweep can call it Current); any difference is not.
    #[test]
    fn canonical_render_is_recognized_byte_for_byte() {
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\ncolor: blue\n---\nBODY\n";
        // The exact canonical render matches…
        let (rendered, _h) = render::render_with_hash(&agent, raw, "codex").unwrap();
        assert!(bytes_match_render(
            &agent,
            raw,
            "codex",
            rendered.as_bytes()
        ));
        // …a hand-edited / different file does not.
        assert!(!bytes_match_render(
            &agent,
            raw,
            "codex",
            b"different bytes"
        ));
        // Identity tool (claude-code ships the source verbatim) also matches.
        let (raw_render, _h2) = render::render_with_hash(&agent, raw, "claudeCode").unwrap();
        assert!(bytes_match_render(
            &agent,
            raw,
            "claudeCode",
            raw_render.as_bytes()
        ));
    }

    /// Track records provenance but must NOT create or touch any file.
    #[tokio::test]
    async fn track_writes_no_file() {
        let home = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\n---\nBODY\n";

        let rec = track_agent_record(
            &agent,
            raw,
            "codex",
            home.path(),
            None,
            "src-1",
            "body-1",
            "v1",
            "2026-06-06T00:00:00Z",
        )
        .unwrap();

        let path = home
            .path()
            .join(".codex/agents")
            .join("frontend-developer.toml");
        assert!(!path.exists(), "Track must not write the agent file");
        assert_eq!(
            PathBuf::from(&rec.dest),
            path,
            "record points at the canonical dest"
        );

        // The recorded rendered_hash equals a real render — so if the user's file
        // happens to match it, reconcile yields Current; otherwise Modified.
        let (_b, render_hash) = render::render_with_hash(&agent, raw, "codex").unwrap();
        assert_eq!(rec.rendered_hash, render_hash);
        assert_eq!(
            classify(
                Some(&render_hash),
                &rec.rendered_hash,
                &rec.source_hash,
                Some("src-1")
            ),
            InstallState::Current,
            "a tracked file that matches the canonical render reconciles as Current"
        );
        assert_eq!(
            classify(
                Some("hand-edited"),
                &rec.rendered_hash,
                &rec.source_hash,
                Some("src-1")
            ),
            InstallState::Modified,
            "a tracked file that differs reconciles as Modified (never silently clobbered)"
        );
    }

    #[tokio::test]
    async fn tracked_conversion_slug_update_reuses_existing_destination() {
        let home = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let mut agent = sample_agent();
        agent.slug = "engineering-frontend-developer".into();
        let raw = "---\nname: Frontend Developer\ndescription: Builds UIs.\n---\nBODY\n";
        let conversion_dest = home
            .path()
            .join(".codex/agents")
            .join("frontend-developer.toml");
        std::fs::create_dir_all(conversion_dest.parent().unwrap()).unwrap();
        std::fs::write(&conversion_dest, b"OLDER CLI OUTPUT").unwrap();

        let tracked = track_agent_record(
            &agent,
            raw,
            "codex",
            home.path(),
            None,
            "src-1",
            "body-1",
            "v1",
            "2026-06-12T00:00:00Z",
        )
        .unwrap();
        assert_eq!(PathBuf::from(&tracked.dest), conversion_dest);

        write_agent_files_to(
            &agent,
            raw,
            "codex",
            home.path(),
            None,
            Some(backups.path()),
            "src-2",
            "body-2",
            "v2",
            "2026-06-12T01:00:00Z",
            Some(&conversion_dest),
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&conversion_dest).unwrap(),
            render::render(&agent, raw, "codex").unwrap()
        );
        assert!(
            !home
                .path()
                .join(".codex/agents/engineering-frontend-developer.toml")
                .exists(),
            "update must not create a duplicate source-slug file"
        );
    }

    /// A write that overwrites an existing, DIFFERENT file must preserve the old
    /// bytes in the backups dir first; an identical (no-op) write must not.
    #[tokio::test]
    async fn write_backs_up_existing_differing_file() {
        let home = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let dest = home.path().join(".codex/agents/frontend-developer.toml");

        // Simulate a user-edited file already on disk at the dest.
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"USER EDITED CONTENT").unwrap();

        // Update over it (with backups enabled).
        write_agent_files(
            &agent,
            "---\nname: Frontend Developer\n---\nNEW\n",
            "codex",
            home.path(),
            None,
            Some(backups.path()),
            "src-2",
            "body-2",
            "v2",
            "2026-06-06T01:02:03Z",
        )
        .await
        .unwrap();

        // The old bytes were preserved before the overwrite.
        let saved: Vec<_> = std::fs::read_dir(backups.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| std::fs::read(e.path()).unwrap())
            .collect();
        assert_eq!(saved.len(), 1, "exactly one backup created");
        assert_eq!(
            saved[0], b"USER EDITED CONTENT",
            "backup holds the pre-overwrite bytes"
        );

        // A second, byte-identical write makes no new backup (not destructive).
        let before = std::fs::read(&dest).unwrap();
        write_agent_files(
            &agent,
            "---\nname: Frontend Developer\n---\nNEW\n",
            "codex",
            home.path(),
            None,
            Some(backups.path()),
            "src-2",
            "body-2",
            "v2",
            "2026-06-06T02:02:03Z",
        )
        .await
        .unwrap();
        let after = std::fs::read(&dest).unwrap();
        assert_eq!(before, after, "identical render leaves the file unchanged");
        let count = std::fs::read_dir(backups.path()).unwrap().count();
        assert_eq!(count, 1, "no-op write adds no backup");
    }

    #[tokio::test]
    async fn uninstall_canonical_file_needs_no_backup() {
        let home = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\ndescription: Builds UIs.\n---\nBODY\n";
        let dest = home.path().join(".codex/agents/frontend-developer.toml");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, render::render(&agent, raw, "codex").unwrap()).unwrap();

        remove_agent_files(
            &agent,
            raw,
            "codex",
            home.path(),
            None,
            None,
            backups.path(),
            "2026-06-12T00:00:00Z",
        )
        .await
        .unwrap();

        assert!(!dest.exists());
        assert_eq!(std::fs::read_dir(backups.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn uninstall_modified_file_backs_up_before_delete() {
        let home = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\ndescription: Builds UIs.\n---\nBODY\n";
        let dest = home.path().join(".codex/agents/frontend-developer.toml");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"USER MODIFIED").unwrap();

        remove_agent_files(
            &agent,
            raw,
            "codex",
            home.path(),
            None,
            None,
            backups.path(),
            "2026-06-12T00:00:00Z",
        )
        .await
        .unwrap();

        assert!(!dest.exists());
        let saved: Vec<_> = std::fs::read_dir(backups.path())
            .unwrap()
            .map(|entry| std::fs::read(entry.unwrap().path()).unwrap())
            .collect();
        assert_eq!(saved, vec![b"USER MODIFIED".to_vec()]);
    }

    #[tokio::test]
    async fn uninstall_missing_file_is_successful() {
        let home = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        remove_agent_files(
            &sample_agent(),
            "---\nname: Frontend Developer\n---\nBODY\n",
            "codex",
            home.path(),
            None,
            None,
            backups.path(),
            "2026-06-12T00:00:00Z",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn uninstall_copilot_removes_both_destinations() {
        let home = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\n---\nBODY\n";
        for dest in render::dests("copilot", &agent.slug, home.path(), None).unwrap() {
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::write(dest, raw).unwrap();
        }

        remove_agent_files(
            &agent,
            raw,
            "copilot",
            home.path(),
            None,
            None,
            backups.path(),
            "2026-06-12T00:00:00Z",
        )
        .await
        .unwrap();

        for dest in render::dests("copilot", &agent.slug, home.path(), None).unwrap() {
            assert!(!dest.exists());
        }
    }

    #[tokio::test]
    async fn uninstall_removes_orphaned_skill_dir() {
        // #60: skill-md tools install each agent as `<slug>/SKILL.md`. Uninstall
        // must remove the whole `<slug>/` dir — a leftover empty dir is otherwise
        // re-surfaced by the reconcile scan as an untracked phantom.
        let home = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\ndescription: Builds UIs.\n---\nBODY\n";

        for tool in ["osaurus"] {
            let dests = candidate_dests(&agent, raw, tool, home.path(), None).unwrap();
            for dest in &dests {
                std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
                std::fs::write(dest, render::render(&agent, raw, tool).unwrap()).unwrap();
            }

            remove_agent_files(
                &agent,
                raw,
                tool,
                home.path(),
                None,
                None,
                backups.path(),
                "2026-06-12T00:00:00Z",
            )
            .await
            .unwrap();

            for dest in &dests {
                assert!(!dest.exists(), "{tool}: SKILL.md still present at {dest:?}");
                let slug_dir = dest.parent().unwrap();
                assert!(
                    !slug_dir.exists(),
                    "{tool}: orphaned skill dir left at {slug_dir:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn uninstall_backup_failure_preserves_original() {
        let home = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let backup_path = scratch.path().join("not-a-directory");
        std::fs::write(&backup_path, b"occupied").unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\n---\nBODY\n";
        let dest = home.path().join(".codex/agents/frontend-developer.toml");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"USER MODIFIED").unwrap();

        assert!(remove_agent_files(
            &agent,
            raw,
            "codex",
            home.path(),
            None,
            None,
            &backup_path,
            "2026-06-12T00:00:00Z",
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"USER MODIFIED");
    }

    #[tokio::test]
    async fn uninstall_removal_failure_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("directory");
        std::fs::create_dir(&directory).unwrap();
        assert!(remove_file_strict(&directory).await.is_err());
        assert!(directory.exists());
    }

    #[test]
    fn ledger_json_roundtrips() {
        let recs = vec![InstallRecord {
            slug: "a".into(),
            tool: "cursor".to_string(),
            scope: crate::types::Scope::Project,
            project_path: Some("/p".into()),
            dest: "/p/.cursor/rules/a.mdc".into(),
            dests: vec!["/p/.cursor/rules/a.mdc".into()],
            file_hashes: std::collections::BTreeMap::new(),
            aggregate_slugs: Vec::new(),
            source_hash: "sh".into(),
            body_hash: "bh".into(),
            rendered_hash: "rh".into(),
            installed_at: "2026-06-05T00:00:00Z".into(),
            corpus_version: "v".into(),
            domain: InstallDomain::Agent,
            plugin_registered: None,
        }];
        let bytes = serde_json::to_vec(&recs).unwrap();
        // tool serializes camelCase per the wire contract.
        assert!(String::from_utf8_lossy(&bytes).contains("\"cursor\""));
        let back: Vec<InstallRecord> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].tool, "cursor");
    }

    // ---------- TASK 13: the ledger domain discriminator ----------

    /// A rule ledger row. `tool` holds the HARNESS id, `slug` the rule slug —
    /// no parallel field pairs (spec §5.4).
    fn rule_row(slug: &str, harness: &str, project: Option<&str>) -> InstallRecord {
        let mut record = row(slug, harness, project);
        record.dest = format!("/dest/{slug}.md");
        record.dests = vec![record.dest.clone()];
        record.domain = InstallDomain::Rule;
        record
    }

    /// A ledger with both domains, including the collision case ruling C4 warns
    /// about: a rule row whose harness id is byte-equal to an agent tool id.
    fn mixed_ledger() -> Vec<InstallRecord> {
        vec![
            row("shared-slug", "claudeCode", None),
            row("antigravity-agent", "antigravity", None),
            row("opencode-agent", "opencode", None),
            row("project-agent", "cursor", Some("/p")),
            rule_row("shared-slug", "antigravity", None),
            rule_row("typescript", "opencode", None),
            rule_row("python", "oh-my-pi", Some("/p")),
        ]
    }

    /// The regression gate from spec §5.4 / TASK 13: a v0.8.x ledger parses and
    /// re-serializes BYTE-IDENTICALLY. Drives `parse_ledger`, the real load
    /// path, so the `dests` backfill cannot drift away from this assertion.
    ///
    /// The fixture's first two rows are verbatim rows from a real app-data
    /// `installs.json` (written by v0.8.x, home path rewritten to
    /// `/Users/example`); the last three are real-shaped additions for the
    /// shapes that ledger could not supply (a multi-file `dests` +
    /// `fileHashes` row, an `aggregateSlugs` roster row, and a project-scoped
    /// row), because those are exactly the fields the discriminator must not
    /// disturb.
    #[test]
    fn v0_8_x_ledger_round_trips_byte_identically() {
        const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/installs-v0.8.x.json");
        let records = parse_ledger(FIXTURE).expect("a v0.8.x ledger must still parse");

        assert_eq!(records.len(), 5, "every fixture row parses");
        assert!(
            records.iter().all(|r| r.domain == InstallDomain::Agent),
            "a row without a `domain` key is an agent row"
        );
        assert!(
            records.iter().all(|r| r.plugin_registered.is_none()),
            "the plugin flag is absent on every old row"
        );

        let out = serde_json::to_vec_pretty(&records).expect("serialize");
        assert_eq!(
            out, FIXTURE,
            "agent rows must serialize byte-identically (no `domain` key, no reordering)"
        );
    }

    /// A row written before multi-file support has no `dests` and no `domain`:
    /// both defaults must land without a migration step.
    #[test]
    fn parse_ledger_backfills_dests_and_defaults_the_domain() {
        let raw = br#"[{"slug":"a","tool":"cursor","scope":"user","projectPath":null,
            "dest":"/p/a.mdc","sourceHash":"s","bodyHash":"b","renderedHash":"r",
            "installedAt":"t","corpusVersion":"v"}]"#;
        let records = parse_ledger(raw).expect("an old row parses");
        assert_eq!(records[0].dests, vec!["/p/a.mdc".to_string()]);
        assert_eq!(records[0].domain, InstallDomain::Agent);
        assert!(records[0].plugin_registered.is_none());
    }

    #[test]
    fn parse_ledger_reads_the_domain_discriminator() {
        let mut rule = rule_row("typescript", "opencode", None);
        rule.plugin_registered = Some(true);
        let raw = serde_json::to_vec_pretty(&vec![rule]).expect("serialize");
        let records = parse_ledger(&raw).expect("parses");
        assert_eq!(records[0].domain, InstallDomain::Rule);
        assert_eq!(records[0].plugin_registered, Some(true));
        assert_eq!(records[0].tool, "opencode", "the harness id survives");
    }

    #[test]
    fn a_rule_row_serializes_its_domain_and_an_agent_row_does_not() {
        let agent = serde_json::to_string(&row("a", "claudeCode", None)).expect("serialize");
        assert!(!agent.contains("domain"), "agent row: {agent}");
        assert!(!agent.contains("pluginRegistered"), "agent row: {agent}");

        let rule =
            serde_json::to_string(&rule_row("typescript", "opencode", None)).expect("serialize");
        assert!(rule.contains("\"domain\":\"rule\""), "rule row: {rule}");

        // The OpenCode registration flag is a separate boolean, never a path
        // (ruling C2), and only appears when a caller sets it.
        let mut registered = rule_row("typescript", "opencode", None);
        registered.plugin_registered = Some(true);
        let json = serde_json::to_string(&registered).expect("serialize");
        assert!(json.contains("\"pluginRegistered\":true"), "{json}");
        assert!(!json.contains("opencode.json"), "{json}");
    }

    #[test]
    fn rule_rows_append_replace_remove_and_list() {
        let mut ledger = vec![row("agent-a", "claudeCode", None)];
        upsert_rule_row(&mut ledger, rule_row("typescript", "opencode", None));
        upsert_rule_row(&mut ledger, rule_row("python", "antigravity", None));
        assert_eq!(rule_rows(&ledger).len(), 2);
        assert_eq!(agent_rows(&ledger).count(), 1);

        // Re-installing the same (slug, harness, project) replaces the row.
        // A caller that forgets the domain cannot smuggle in an agent row.
        let mut second = rule_row("typescript", "opencode", None);
        second.domain = InstallDomain::Agent;
        second.rendered_hash = "second".into();
        upsert_rule_row(&mut ledger, second);
        assert_eq!(rule_rows(&ledger).len(), 2, "replace, never append");
        let replaced = rule_rows(&ledger)
            .into_iter()
            .find(|r| r.slug == "typescript")
            .expect("typescript row");
        assert_eq!(replaced.rendered_hash, "second");
        assert_eq!(replaced.domain, InstallDomain::Rule);

        // A different scope is a different install.
        upsert_rule_row(&mut ledger, rule_row("typescript", "opencode", Some("/p")));
        assert_eq!(rule_rows(&ledger).len(), 3);

        assert!(remove_rule_row(&mut ledger, "typescript", "opencode", None));
        assert!(
            !remove_rule_row(&mut ledger, "typescript", "opencode", None),
            "removing a missing row reports false"
        );
        assert_eq!(rule_rows(&ledger).len(), 2);
        assert_eq!(agent_rows(&ledger).count(), 1, "agent rows untouched");
    }

    /// Ruling C4, surface 1/6: `installs_reconcile` iterates `agent_rows`, so a
    /// rule row can never become an `InstalledAgent` (and the Foreign sweep
    /// never sees a rule project root).
    #[test]
    fn installs_reconcile_ignores_rule_rows() {
        let ledger = mixed_ledger();
        let rows: Vec<&InstallRecord> = agent_rows(&ledger).collect();
        assert_eq!(rows.len(), 4, "only the four agent rows");
        assert!(rows.iter().all(|r| r.domain == InstallDomain::Agent));
        assert!(
            !rows.iter().any(|r| r.slug == "typescript"),
            "a rule row must not surface as an installed agent"
        );
        assert_eq!(
            rows.iter().filter(|r| r.tool == "antigravity").count(),
            1,
            "the rule row's `antigravity` harness id must not double the count"
        );
    }

    /// Ruling C4, surface 2/6: the persona detail panel shows agent rows only,
    /// even when a rule row shares the slug.
    #[test]
    fn installs_for_agent_ignores_rule_rows() {
        let ledger = mixed_ledger();
        let installs = agent_installs_for(&ledger, "shared-slug");
        assert_eq!(installs.len(), 1, "the agent row only: {installs:?}");
        assert_eq!(installs[0].tool, "claudeCode");
        assert!(agent_installs_for(&ledger, "typescript").is_empty());
    }

    /// Ruling C4, surface 3/6: `tools_list` counts. The rule row whose `tool` is
    /// the harness id `antigravity`/`opencode` is byte-equal to the agent tool
    /// id — this is the silent double-count the ruling exists for.
    #[test]
    fn tools_list_does_not_double_count_rule_rows() {
        let ledger = mixed_ledger();
        assert_eq!(tool_installed_count(&ledger, "antigravity"), 1);
        assert_eq!(tool_installed_count(&ledger, "opencode"), 1);
        assert_eq!(tool_installed_count(&ledger, "claudeCode"), 1);
        // A harness with no agent row at all stays at zero.
        assert_eq!(tool_installed_count(&ledger, "oh-my-pi"), 0);
    }

    /// Ruling C4, surface 4/6: `projects_list` must not report a rule install as
    /// an agent install in that project (spec §10.8).
    #[test]
    fn projects_list_ignores_rule_rows() {
        let ledger = mixed_ledger();
        let counts = project_install_counts(&ledger);
        assert_eq!(
            counts.get("/p").copied(),
            Some(1),
            "one agent row in /p; the rule row in /p is not an agent install: {counts:?}"
        );
        assert_eq!(counts.len(), 1);
    }

    /// Ruling C4, surface 5/6: the Agentfile carries agent entries only, and an
    /// agent entry gains no `domain` key (a v0.8.x Agentfile stays byte-stable).
    #[test]
    fn loadout_export_never_writes_rule_entries() {
        let ledger = mixed_ledger();
        let af = agentfile_from(&ledger);
        assert_eq!(af.installs.len(), 4);
        assert!(af.installs.iter().all(|e| e.domain == InstallDomain::Agent));

        let bytes = serde_json::to_string_pretty(&af).expect("serialize");
        assert!(!bytes.contains("oh-my-pi"), "no harness id leaks: {bytes}");
        assert!(!bytes.contains("typescript"), "{bytes}");
        assert!(!bytes.contains("domain"), "{bytes}");
    }

    /// Ruling C4, surface 6/6: import restores agent entries only. A rule entry
    /// would reach `do_install` with a harness id as `tool` and vanish.
    #[test]
    fn loadout_import_skips_rule_entries() {
        let af = Agentfile {
            agentfile: 1,
            installs: vec![
                LoadoutEntry {
                    slug: "agent-a".into(),
                    tool: "claudeCode".to_string(),
                    project_path: None,
                    domain: InstallDomain::Agent,
                },
                LoadoutEntry {
                    slug: "typescript".into(),
                    tool: "opencode".to_string(),
                    project_path: None,
                    domain: InstallDomain::Rule,
                },
            ],
        };
        // The wire form carries the discriminator, and only for a rule entry.
        let bytes = serde_json::to_vec(&af).expect("serialize");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("\"domain\":\"rule\""), "{text}");
        assert_eq!(text.matches("\"domain\"").count(), 1, "{text}");

        let entries = agent_loadout_entries(serde_json::from_slice(&bytes).expect("parse"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].slug, "agent-a");

        // An Agentfile written before M1 (no `domain` key) imports as agents.
        let legacy: Agentfile =
            serde_json::from_str(r#"{"agentfile":1,"installs":[{"slug":"a","tool":"cursor"}]}"#)
                .expect("legacy Agentfile parses");
        assert_eq!(agent_loadout_entries(legacy).len(), 1);
    }

    /// Ruling C4 extended to the MUTATING agent paths, which the ruling's table
    /// does not list: install, track, update, diff and uninstall all key on
    /// `(slug, tool, project)` and would otherwise rewrite or delete a rule row
    /// whose harness id is byte-equal to an agent tool id. `project_forget` is
    /// the same hazard for a rule installed into a project.
    #[test]
    fn agent_mutations_never_touch_rule_rows() {
        let ledger = mixed_ledger();
        assert!(
            !ledger
                .iter()
                .any(|r| matches_agent_install(r, "shared-slug", "antigravity", None)),
            "an agent install must not adopt the rule row with the same slug + harness id"
        );
        assert!(ledger
            .iter()
            .any(|r| matches_agent_install(r, "shared-slug", "claudeCode", None)));

        let mut rows = mixed_ledger();
        prune_project_rows(&mut rows, "/p");
        assert!(
            rows.iter()
                .any(|r| r.domain == InstallDomain::Rule && r.project_path.as_deref() == Some("/p")),
            "forgetting a project must not drop a rule install row"
        );
        assert!(
            !rows.iter().any(
                |r| r.domain == InstallDomain::Agent && r.project_path.as_deref() == Some("/p")
            ),
            "the project's agent row is gone"
        );
    }

    /// Ruling C2: `opencode.json` may never be a tracked destination, because
    /// `rules_uninstall` deletes every path in `dests`.
    #[test]
    fn a_rule_row_may_never_track_the_opencode_config() {
        let config = "/home/me/.config/opencode/opencode.json";
        let mut record = rule_row("typescript", "opencode", None);
        assert!(check_tracked_destinations(&record).is_ok());

        record.dest = config.into();
        let err = check_tracked_destinations(&record).expect_err("dest is refused");
        match err {
            AppError::InvalidArgument { message } => {
                assert!(message.contains(config), "{message}");
                assert!(
                    message.contains("never a tracked install destination"),
                    "{message}"
                );
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }

        // Same guard for a secondary destination.
        let mut secondary = rule_row("typescript", "opencode", None);
        secondary.dests = vec![
            "/home/me/.config/opencode/rules/typescript.md".into(),
            config.into(),
        ];
        assert!(check_tracked_destinations(&secondary).is_err());
    }

    #[test]
    fn ensure_installable_rejects_unported_tools_and_unknown_ids() {
        // Catalog formats with no renderer here. Before the gate, all three
        // wrote real files while `supported()` omitted them (issue #49).
        for tool in ["kimi", "openclaw", "vibe", "aider", "windsurf"] {
            assert!(ensure_installable(tool).is_err(), "{tool} must be refused");
        }
        // Hermes is a plugin, never per-agent installable.
        assert!(ensure_installable("hermes").is_err());

        for tool in [
            "claudeCode",
            "codex",
            "geminiCli",
            "antigravity",
            "zcode",
            "deepseekHarness",
        ] {
            assert!(ensure_installable(tool).is_ok(), "{tool} must be allowed");
        }
        assert!(
            ensure_installable("not-a-tool").is_err(),
            "unknown ids are refused"
        );
    }

    /// The gate has to fire before the first byte reaches the filesystem, not
    /// after a render succeeds — that ordering is what the bug turned on.
    #[tokio::test]
    async fn write_agent_files_refuses_without_touching_disk() {
        let home = tempfile::tempdir().unwrap();
        let agent = sample_agent();
        let raw = "---\nname: Frontend Developer\n---\nORIGINAL\n";

        for tool in ["kimi", "openclaw", "vibe"] {
            let result = write_agent_files(
                &agent,
                raw,
                tool,
                home.path(),
                None,
                None,
                "src-1",
                "body-1",
                "v1",
                "2026-06-05T00:00:00Z",
            )
            .await;
            assert!(result.is_err(), "{tool} must be refused before writing");
        }

        let leftovers: Vec<_> = std::fs::read_dir(home.path()).unwrap().collect();
        assert!(
            leftovers.is_empty(),
            "gate must fail before any write; found {leftovers:?}"
        );
    }
}
