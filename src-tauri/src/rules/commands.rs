//! Tauri command surface for the rules subsystem (TASK 14–17, W6).
//!
//! Commands, all `async` because every one either reads the corpus or reaches
//! `tokio::fs`:
//!
//! | Command | Role |
//! | --- | --- |
//! | [`rules_list`] | rule summaries from the index, bodies omitted |
//! | [`rules_get`] | one full [`Rule`] by slug (`relative_path`, slug ≠ filename) |
//! | [`rules_preview`] | compiled bytes + destination per harness, **no writes** |
//! | [`rules_install`] | render → dests → write → ledger row |
//! | [`rules_uninstall`] | delete exactly the recorded files, then the row |
//! | [`rules_installs`] | the rule ledger rows (restart truth) |
//! | [`rules_status`] | seed + `{ configured, count, version, emptyReason }` |
//! | [`rules_refresh`] | fetch the tarball, swap, re-index |
//!
//! ## Why the orchestration lives in pure-ish helpers
//!
//! Every command resolves its inputs (app-data root, corpus root, user base)
//! and then delegates to a function that takes plain paths:
//! [`list_summaries`], [`read_rule`], [`preview_rule`], [`install_rule`],
//! [`remove_rule_files`]. That split is the only way to prove the load-bearing
//! properties — zero writes on refusal, byte equality, exact-file deletion —
//! without standing up a Tauri `AppHandle` and the OS app-data directory.
//!
//! ## Non-negotiable ordering (spec §8, rulings A6/C2/C11)
//!
//! `rules_install` is fail-closed and ordered. Each gate returns **before the
//! first write**:
//!
//! ```text
//! slug regex → harness known → harness installable → project root present
//!   → frontmatter valid → read body → render_rule_files (C11 blockers)
//!   → rule_dests (slug re-validated) → create parent dirs → backup
//!   → opencode plugin registration (opencode only) → atomic write
//!   → ledger row
//! ```
//!
//! Ruling A6 and architecture §4 put plugin registration **before** the rule
//! file write: otherwise a successful rule write plus a failed plugin write
//! leaves an inert rule with no error path. A plugin entry with no rule is
//! harmless, and the merge is idempotent, so a retry repairs it. `opencode.json`
//! is never a tracked destination (ruling C2): it is absent from `dest`/`dests`
//! and uninstall never deletes it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

use tauri::{AppHandle, State};

use crate::corpus::{app_data_dir, resource_dir};
use crate::error::AppError;
use crate::install;
use crate::render::{scope_for, sha256_hex};
use crate::rules::corpus as rules_corpus;
use crate::rules::opencode;
use crate::rules::parse::parse_rule;
use crate::rules::registry;
use crate::rules::render::{harness_base, render_rule_files, rule_dests, validate_slug};
use crate::state::AppState;
use crate::types::{
    InstallDomain, InstallRecord, Rule, RuleEntry, RulePreview, RulePreviewEntry, RuleStatus,
    RuleSummary, Scope,
};

/// The bundled rules baseline: `<resources>/resources/rules-baseline`, whose
/// `rules/` subtree is copied into the working copy on first launch (TASK 12).
fn rules_baseline_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    Ok(resource_dir(app)?.join("resources").join("rules-baseline"))
}

// =========================================================
// Corpus access
// =========================================================

/// Seed on first use, then load the corpus root and its index.
///
/// Commands must not depend on `rules_status` having run: a fresh install can
/// invoke `rules_list` first, and the baseline must appear with no network
/// (TASK 14). A seed failure is logged and the index build reports the real
/// state — exactly what `corpus::status` does, so the error a caller sees names
/// the path that is actually broken.
async fn load_index(app: &AppHandle) -> Result<(PathBuf, BTreeMap<String, RuleEntry>), AppError> {
    let app_data = app_data_dir(app)?;
    let baseline = rules_baseline_dir(app)?;
    if let Err(error) = rules_corpus::seed_if_empty(&app_data, &baseline).await {
        tracing::warn!("rules: seed from baseline failed: {error}");
    }
    let root = rules_corpus::rules_corpus_dir(&app_data);
    let index = rules_corpus::build_rules_index(&root)?;
    Ok((root, index))
}

/// The `AppError` for a slug no corpus row declares (mirrors `corpus_get`).
fn unknown_rule(slug: &str) -> AppError {
    AppError::InvalidArgument {
        message: format!("unknown rule: {slug}"),
    }
}

fn require_entry<'a>(
    index: &'a BTreeMap<String, RuleEntry>,
    slug: &str,
) -> Result<&'a RuleEntry, AppError> {
    index.get(slug).ok_or_else(|| unknown_rule(slug))
}

/// Join a corpus-relative source path, refusing anything that is not a plain
/// relative path.
///
/// `relative_path` is produced by our own scanner, so this is defense in depth:
/// the value crosses the IPC boundary in `RuleSummary`/`Rule`, and a corrupted
/// index must not be able to point a read outside the corpus root.
fn resolve_source(corpus_root: &Path, relative_path: &str) -> Result<PathBuf, AppError> {
    let relative = Path::new(relative_path);
    let plain = !relative.is_absolute()
        && relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !plain {
        return Err(AppError::InvalidArgument {
            message: format!("invalid rule source path: {relative_path:?}"),
        });
    }
    Ok(corpus_root.join(relative))
}

/// Read + parse one rule's file, using the index row's `relative_path`.
///
/// This is why slug ≠ filename is not a problem: `rules/base/commit-convention.md`
/// declares `name: commit-conventions`, and the index row is the only place the
/// slug → file mapping survives (ruling C6).
fn read_rule(corpus_root: &Path, entry: &RuleEntry) -> Result<Rule, AppError> {
    validate_slug(&entry.slug)?;
    let path = resolve_source(corpus_root, &entry.relative_path)?;
    let bytes = std::fs::read(&path).map_err(|e| AppError::Io {
        message: format!("read {}: {e}", path.display()),
    })?;
    let rule = parse_rule(&entry.relative_path, &bytes)?;
    if rule.slug != entry.slug {
        return Err(AppError::Internal {
            message: format!(
                "{} declares name '{}' but the index keys it as '{}'; the index is stale",
                entry.relative_path, rule.slug, entry.slug
            ),
        });
    }
    Ok(rule)
}

// =========================================================
// Pure view-model helpers
// =========================================================

fn summary_of(entry: &RuleEntry) -> RuleSummary {
    RuleSummary {
        slug: entry.slug.clone(),
        name: entry.name.clone(),
        description: entry.description.clone(),
        category: entry.category.clone(),
        relative_path: entry.relative_path.clone(),
        globs: entry.globs.clone(),
        always_apply: entry.always_apply,
        trigger: entry.trigger.clone(),
        match_: entry.match_.clone(),
        tags: entry.tags.clone(),
        tools: entry.tools.clone(),
        validation: entry.validation.clone(),
    }
}

/// `rules_list`'s payload: every row (invalid ones included), deterministically
/// ordered by `category`, then `name`, then `slug` (TASK 14).
///
/// An invalid rule is **listed** with its `validation.errors`; only preview and
/// install refuse it (spec §10.4 — a visible error, not an invisible rule).
pub(crate) fn list_summaries(
    index: &BTreeMap<String, RuleEntry>,
    category: Option<&str>,
) -> Vec<RuleSummary> {
    let mut rows: Vec<RuleSummary> = index
        .values()
        .filter(|entry| match category {
            Some(wanted) => entry.category == wanted,
            None => true,
        })
        .map(summary_of)
        .collect();
    rows.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.slug.cmp(&b.slug))
    });
    rows
}

/// The verbatim validation error the UI must display (spec §8: "surfaced
/// verbatim"). No rewording, no truncation, no prefix.
fn invalid_rule(entry: &RuleEntry) -> AppError {
    let message = if entry.validation.errors.is_empty() {
        format!("rule '{}' failed validation", entry.slug)
    } else {
        entry.validation.errors.join("\n")
    };
    AppError::InvalidArgument { message }
}

fn ensure_valid(entry: &RuleEntry) -> Result<(), AppError> {
    if entry.validation.valid {
        Ok(())
    } else {
        Err(invalid_rule(entry))
    }
}

// =========================================================
// Preview
// =========================================================

/// Compile `entry` for every **installable** harness without touching disk.
///
/// Each entry's `bytes` come from [`render_rule_files`] — the same function the
/// installer calls — so preview and install cannot diverge (TASK 15's equality
/// test). Ruling C11's blockers propagate as an error: a rule this emitter
/// cannot reproduce byte-for-byte is refused rather than previewed with bytes
/// that disagree with upstream.
///
/// `dest` is computed and never created. `project_root` selects project scope
/// and replaces the user base; `None` is user scope.
pub(crate) fn preview_rule(
    corpus_root: &Path,
    entry: &RuleEntry,
    scope: Scope,
    project_root: Option<&Path>,
    tool_paths: &HashMap<String, String>,
    os_home: &Path,
) -> Result<RulePreview, AppError> {
    ensure_valid(entry)?;
    let project_root = match scope {
        Scope::Project => Some(require_project_root(project_root.ok_or_else(|| {
            AppError::InvalidArgument {
                message: "project scope requires a project path".into(),
            }
        })?)?),
        Scope::User => None,
    };
    let rule = read_rule(corpus_root, entry)?;

    let mut harnesses = Vec::new();
    for meta in registry::installable() {
        let files = render_rule_files(&rule, &meta.id)?;
        let bytes = files
            .into_iter()
            .next()
            .map(|file| file.bytes)
            .unwrap_or_default();
        let rendered_hash = sha256_hex(bytes.as_bytes());
        let base = harness_base(tool_paths, &meta.id, os_home);
        let dests = rule_dests(&meta.id, &rule.slug, &base, project_root)?;
        let dest = dests.into_iter().next().ok_or_else(|| AppError::Internal {
            message: format!("rule harness '{}' resolved no destination", meta.id),
        })?;
        harnesses.push(RulePreviewEntry {
            harness: meta.id.clone(),
            label: meta.label.clone(),
            bytes,
            dest: dest.to_string_lossy().into_owned(),
            rendered_hash,
            installable: meta.installable(),
        });
    }

    Ok(RulePreview {
        slug: rule.slug,
        scope,
        project_path: project_root.map(|root| root.to_string_lossy().into_owned()),
        harnesses,
    })
}

// =========================================================
// Install
// =========================================================

/// Everything `install_rule` needs, already resolved by the command.
///
/// A struct rather than eight positional arguments: the paths are easy to swap
/// (home vs project root vs corpus root), and swapping two of them would write
/// a rule to the wrong tree — exactly the class of bug the dest resolver exists
/// to prevent.
pub(crate) struct RuleInstallRequest<'a> {
    pub corpus_root: &'a Path,
    pub entry: &'a RuleEntry,
    pub harness: &'a str,
    pub scope: Scope,
    pub project_root: Option<&'a Path>,
    pub tool_paths: &'a HashMap<String, String>,
    pub os_home: &'a Path,
    pub backups: &'a Path,
    pub corpus_version: &'a str,
    pub installed_at: &'a str,
}

/// Write one rule for one harness, then return the ledger row that records it.
///
/// The ledger row is **returned, not persisted** — `append_rule_install` does
/// that as the last step of the command. That split is deliberate: a failure
/// anywhere in this function leaves no row claiming a file that is not there.
///
/// Side effects: the rule file (atomic write), a backup of a divergent
/// pre-existing file, the OpenCode plugin entry when `harness == "opencode"`
/// (ruling A6), and the parent directories a nested destination needs.
pub(crate) async fn install_rule(
    request: RuleInstallRequest<'_>,
) -> Result<InstallRecord, AppError> {
    let RuleInstallRequest {
        corpus_root,
        entry,
        harness,
        scope,
        project_root,
        tool_paths,
        os_home,
        backups,
        corpus_version,
        installed_at,
    } = request;

    // 0. A project root must be absolute and real before it is joined into a
    // destination (spec §7): a relative root would resolve against the process
    // CWD and store a relative `dest` in the ledger.
    if scope == Scope::Project {
        let root = project_root.ok_or_else(|| AppError::InvalidArgument {
            message: "project scope requires a project path".into(),
        })?;
        require_project_root(root)?;
    }

    // 1. Slug gate before any path join (spec §8, DENY-03).
    validate_slug(&entry.slug)?;

    // 2. Harness gate: unknown, then "recognised but no renderer" (spec §8).
    let meta = registry::get(harness).ok_or_else(|| AppError::InvalidArgument {
        message: format!("unknown rule harness: {harness}"),
    })?;
    if !meta.installable() {
        return Err(AppError::InvalidArgument {
            message: format!(
                "rule harness '{harness}' has no renderer in this app (format {})",
                meta.format
            ),
        });
    }

    // 3. Project scope needs a root the user picked — no silent `~` write.
    let project_root = match scope {
        Scope::Project => Some(project_root.ok_or_else(|| AppError::InvalidArgument {
            message: format!(
                "rule harness '{harness}' is project-scoped; a project path is required"
            ),
        })?),
        Scope::User => None,
    };

    // 4. Validation before any render or write.
    ensure_valid(entry)?;

    // 5. Body (and the hashes the ledger records).
    let rule = read_rule(corpus_root, entry)?;

    // 6. Render through the ONE entry point, so C11 blockers propagate.
    let files = render_rule_files(&rule, harness)?;

    // 7. Destinations; `rule_dests` re-validates the slug.
    let base = harness_base(tool_paths, harness, os_home);
    let dests = rule_dests(harness, &rule.slug, &base, project_root)?;
    if files.is_empty() || files.len() != dests.len() {
        return Err(AppError::Internal {
            message: format!(
                "rule harness '{harness}' produced {} file(s) for {} destination(s)",
                files.len(),
                dests.len()
            ),
        });
    }

    // 8. Prepare the filesystem before the plugin write: parent directories are
    //    required by `atomic_write` (it never mkdir_p's), and a backup failure
    //    must abort while nothing has been written yet.
    for dest in &dests {
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Io {
                    message: format!("create {}: {e}", parent.display()),
                })?;
        }
    }
    for (file, dest) in files.iter().zip(dests.iter()) {
        install::backup_if_differs(dest, file.bytes.as_bytes(), backups, installed_at).await?;
    }

    // 9. OpenCode plugin registration, before the rule file lands (ruling A6,
    //    architecture §4). Every other harness never touches `opencode.json`.
    let plugin = if harness == opencode::HARNESS {
        Some(opencode::register_for_install(&base, project_root).await?)
    } else {
        None
    };

    // 10. The rule file(s): atomic, so a crash leaves the prior file or none.
    for (file, dest) in files.iter().zip(dests.iter()) {
        crate::util::fs::atomic_write(dest, file.bytes.as_bytes()).await?;
    }

    // 11. The ledger row. `dests` holds the rule files only — `opencode.json` is
    //     a side-effect write, recorded as `plugin_registered` and never as a
    //     deletable path (ruling C2).
    let dest_strings: Vec<String> = dests
        .iter()
        .map(|dest| dest.to_string_lossy().into_owned())
        .collect();
    let dest = dest_strings.first().cloned().unwrap_or_default();
    let rendered_hash = sha256_hex(files[0].bytes.as_bytes());

    Ok(InstallRecord {
        slug: rule.slug,
        tool: harness.to_string(),
        scope,
        project_path: project_root.map(|root| root.to_string_lossy().into_owned()),
        dest,
        dests: dest_strings,
        file_hashes: BTreeMap::new(),
        aggregate_slugs: Vec::new(),
        source_hash: rule.source_hash,
        body_hash: rule.body_hash,
        rendered_hash,
        installed_at: installed_at.to_string(),
        corpus_version: corpus_version.to_string(),
        domain: InstallDomain::Rule,
        plugin_registered: plugin.as_ref().map(|registered| registered.written),
    })
}

// =========================================================
// Uninstall
// =========================================================

/// What [`remove_rule_files`] did. A missing file is reported, not raised
/// (TASK 17): the user may have deleted it by hand, and the ledger row must
/// still be removable.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RemovalReport {
    pub removed: Vec<String>,
    pub missing: Vec<String>,
}

/// Delete exactly the paths a ledger row recorded — no wildcards, no directory
/// walks, no pruning of the now-empty `{{slug}}/` directory pi-agent leaves
/// behind (ruling A8; spec §10.7 says "file(s)", not directory).
///
/// **The recorded paths are re-verified before anything is deleted.** The
/// ledger is a plain JSON file, so a hand-edited (or corrupted) row must not be
/// able to name an arbitrary victim: each `dest` is only removed when it equals
/// a destination recomputed from `(harness, slug, scope, project root)`. The
/// `opencode.json` config can never be in that set (ruling C2), so this also
/// closes the uninstall half of C2.
///
/// A permission failure returns `Err` and stops the loop, so the caller can
/// leave the ledger row in place and the user can retry rather than losing the
/// record of what was installed.
pub(crate) async fn remove_rule_files(
    record: &InstallRecord,
    tool_paths: &HashMap<String, String>,
    os_home: &Path,
) -> Result<RemovalReport, AppError> {
    let mut report = RemovalReport::default();

    let project_root = match record.scope {
        Scope::Project => {
            let raw = record
                .project_path
                .as_deref()
                .ok_or_else(|| AppError::InvalidArgument {
                    message: format!(
                        "ledger row for '{}' claims project scope without a project path",
                        record.slug
                    ),
                })?;
            Some(require_project_root(Path::new(raw))?.to_path_buf())
        }
        Scope::User => None,
    };
    let base = harness_base(tool_paths, &record.tool, os_home);
    let expected = rule_dests(&record.tool, &record.slug, &base, project_root.as_deref())?;

    let recorded: Vec<&str> = if record.dests.is_empty() {
        vec![record.dest.as_str()]
    } else {
        record.dests.iter().map(String::as_str).collect()
    };

    // Phase 1 — verify every path BEFORE deleting anything. A row that names a
    // file this install could not own aborts the whole operation, so a corrupted
    // ledger can never cost the user a file, not even partially.
    for path in &recorded {
        if !expected
            .iter()
            .any(|candidate| candidate == Path::new(path))
        {
            return Err(AppError::InvalidArgument {
                message: format!(
                    "refusing to uninstall '{}': {path} is not a destination for \
                     harness '{}' (the ledger row is not trustworthy)",
                    record.slug, record.tool
                ),
            });
        }
    }

    // Phase 2 — delete exactly the verified paths.
    for path in recorded {
        match tokio::fs::remove_file(path).await {
            Ok(()) => report.removed.push(path.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                report.missing.push(path.to_string());
            }
            Err(e) => {
                return Err(AppError::Io {
                    message: format!("remove {path}: {e}"),
                })
            }
        }
    }
    Ok(report)
}

/// A project root must be absolute and an existing directory.
///
/// A relative root resolves against the process CWD — the repo in dev, the
/// bundle directory in a packaged app — and the ledger would then record a
/// relative `dest` that a later uninstall resolves against a *different*
/// directory. Spec §7 says the user picks a project root; this is the boundary
/// that enforces it.
pub(crate) fn require_project_root(root: &Path) -> Result<&Path, AppError> {
    if !root.is_absolute() {
        return Err(AppError::InvalidArgument {
            message: format!("project path must be absolute: {:?}", root.display()),
        });
    }
    if !root.is_dir() {
        return Err(AppError::InvalidArgument {
            message: format!("project path is not a directory: {}", root.display()),
        });
    }
    Ok(root)
}

/// Validate the IPC `project_path` argument and turn it into a root.
pub(crate) fn resolve_project_root(
    project_path: Option<&str>,
) -> Result<Option<PathBuf>, AppError> {
    match project_path {
        None => Ok(None),
        Some(raw) => Ok(Some(require_project_root(Path::new(raw))?.to_path_buf())),
    }
}

// =========================================================
// Shared command helpers
// =========================================================

/// The user-scope base inputs: `settings.tool_paths` (ruling A7) and the OS
/// home. Corrupt settings fall back to an empty map, mirroring `tool_home`'s
/// `unwrap_or(os_home)` posture rather than failing a local install.
async fn tool_paths_and_home(
    state: &AppState,
) -> Result<(HashMap<String, String>, PathBuf), AppError> {
    let tool_paths = state
        .settings
        .read()
        .await
        .effective_settings()
        .map(|settings| settings.tool_paths)
        .unwrap_or_default();
    let home = dirs::home_dir().ok_or_else(|| AppError::Io {
        message: "cannot resolve home directory".into(),
    })?;
    Ok((tool_paths, home))
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

// =========================================================
// Commands — corpus reads
// =========================================================

/// `rules_list` — summaries from the index, bodies omitted, ordered by
/// `(category, name, slug)`.
#[tauri::command]
pub async fn rules_list(
    app: AppHandle,
    _state: State<'_, AppState>,
    category: Option<String>,
) -> Result<Vec<RuleSummary>, AppError> {
    let (_, index) = load_index(&app).await?;
    Ok(list_summaries(&index, category.as_deref()))
}

/// `rules_get` — the full rule (body, hashes, validation) for one slug.
#[tauri::command]
pub async fn rules_get(
    app: AppHandle,
    _state: State<'_, AppState>,
    slug: String,
) -> Result<Rule, AppError> {
    let (root, index) = load_index(&app).await?;
    let entry = require_entry(&index, &slug)?;
    read_rule(&root, entry)
}

/// `rules_preview` — compiled bytes and destinations for every installable
/// harness, with the scope that was actually resolved. Writes nothing.
#[tauri::command]
pub async fn rules_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
    project_path: Option<String>,
) -> Result<RulePreview, AppError> {
    let (root, index) = load_index(&app).await?;
    let entry = require_entry(&index, &slug)?;
    let (tool_paths, os_home) = tool_paths_and_home(&state).await?;
    let project_root = resolve_project_root(project_path.as_deref())?;
    preview_rule(
        &root,
        entry,
        scope_for(project_root.as_deref()),
        project_root.as_deref(),
        &tool_paths,
        &os_home,
    )
}

// =========================================================
// Commands — install lifecycle
// =========================================================

/// `rules_install` — write one rule for one harness and record the ledger row.
///
/// The row is persisted **after** the file write, so a failed write cannot leave
/// a row behind. Re-installing the same `(slug, harness, project_path)`
/// replaces the row (`upsert_rule_row`), never duplicates it.
#[tauri::command]
pub async fn rules_install(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
    harness: String,
    project_path: Option<String>,
) -> Result<InstallRecord, AppError> {
    let (root, index) = load_index(&app).await?;
    let entry = require_entry(&index, &slug)?;
    let app_data = app_data_dir(&app)?;
    let (tool_paths, os_home) = tool_paths_and_home(&state).await?;
    let status = rules_corpus::read_status(&app_data).await?;
    let backups = app_data.join("backups");
    let project_root = resolve_project_root(project_path.as_deref())?;

    let record = install_rule(RuleInstallRequest {
        corpus_root: &root,
        entry,
        harness: &harness,
        scope: scope_for(project_root.as_deref()),
        project_root: project_root.as_deref(),
        tool_paths: &tool_paths,
        os_home: &os_home,
        backups: &backups,
        corpus_version: &status.version,
        installed_at: &now_iso(),
    })
    .await?;

    install::append_rule_install(&app, record.clone()).await?;
    Ok(record)
}

/// `rules_uninstall` — delete the files the app wrote and the ledger row.
///
/// Idempotent: no matching row ⇒ `Ok(())` with nothing touched. A missing file
/// is reported, not failed. The OpenCode plugin entry **stays** (ruling A6/C2) —
/// other rules may depend on it, and it is never a tracked destination.
#[tauri::command]
pub async fn rules_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
    harness: String,
    project_path: Option<String>,
) -> Result<(), AppError> {
    validate_slug(&slug)?;

    let rows = install::list_rule_installs(&app).await?;
    let Some(record) = rows.iter().find(|record| {
        record.slug == slug
            && record.tool == harness
            && record.project_path.as_deref() == project_path.as_deref()
    }) else {
        // Nothing recorded: already uninstalled, or never installed here.
        return Ok(());
    };

    // The ledger row is re-verified against a recomputed destination set before
    // anything is deleted, so a hand-edited row cannot name an arbitrary victim.
    let (tool_paths, os_home) = tool_paths_and_home(&state).await?;
    let report = remove_rule_files(record, &tool_paths, &os_home).await?;
    install::remove_rule_install(&app, &slug, &harness, project_path.as_deref()).await?;

    for path in &report.missing {
        tracing::warn!("rules_uninstall: {path} was already gone; row removed");
    }
    if harness == opencode::HARNESS {
        if let Ok(note) = opencode::uninstall_note() {
            tracing::info!("rules_uninstall: {note}");
        }
    }
    Ok(())
}

/// `rules_installs` — the rule rows from the ledger, the source of truth the UI
/// reads after a restart (spec §10.6). Agent rows are excluded by the ledger
/// layer, so an agent install can never be mistaken for a rule install.
#[tauri::command]
pub async fn rules_installs(app: AppHandle) -> Result<Vec<InstallRecord>, AppError> {
    install::list_rule_installs(&app).await
}

// =========================================================
// Commands — corpus lifecycle (W4)
// =========================================================

/// `rules_status()` — seed on first use, then report
/// `{ configured, count, version, emptyReason }`.
///
/// Local-only: no network. A missing corpus is a state (`emptyReason =
/// "notSynced"`), not an error, so the Rules section can always render.
#[tauri::command]
pub async fn rules_status(app: AppHandle) -> Result<RuleStatus, AppError> {
    let app_data = app_data_dir(&app)?;
    let baseline = rules_baseline_dir(&app)?;
    rules_corpus::status(&app_data, &baseline).await
}

/// `rules_refresh()` — fetch the `agency-rules` tarball, atomically swap the
/// working copy, rebuild `rules-index.json`, and return the fresh status.
///
/// Offline Mode is a hard gate (`require_network` first, mirroring
/// `corpus_refresh`), and a second click while one refresh runs fast-fails
/// instead of queuing a duplicate download.
#[tauri::command]
pub async fn rules_refresh(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RuleStatus, AppError> {
    state.require_network("rules_refresh").await?;

    let _flight = match state.rules_refresh_in_flight.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            return Err(AppError::InvalidArgument {
                message: "rules refresh already in progress".into(),
            })
        }
    };

    let app_data = app_data_dir(&app)?;
    rules_corpus::refresh(&app_data).await
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// The vendored conformance fixtures: the oracle for "install writes the
    /// correct bytes".
    fn fixtures_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rules")
    }

    fn fixture(relative: &str) -> Vec<u8> {
        let path = fixtures_root().join(relative);
        fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    /// A rules corpus in an app-data-shaped tempdir, seeded with the named
    /// source fixtures under `rules/base/`.
    fn corpus_with(app_data: &Path, sources: &[&str]) -> PathBuf {
        let root = rules_corpus::rules_corpus_dir(app_data);
        let dir = root.join("rules/base");
        fs::create_dir_all(&dir).expect("create corpus rules dir");
        for name in sources {
            let bytes = fixture(&format!("sources/{name}"));
            fs::write(dir.join(name), bytes).expect("write source fixture");
        }
        root
    }

    fn entry(root: &Path, slug: &str) -> RuleEntry {
        rules_corpus::build_rules_index(root)
            .expect("index the temp corpus")
            .remove(slug)
            .unwrap_or_else(|| panic!("fixture corpus has slug {slug}"))
    }

    fn empty_paths() -> HashMap<String, String> {
        HashMap::new()
    }

    /// Build a request with the fixed test-only corpus version + timestamp.
    ///
    /// Eight arguments is deliberate here and only here: spelling each path
    /// separately is what makes a swapped home/project root visible at the call
    /// site, and the production struct is the thing under test.
    #[allow(clippy::too_many_arguments)]
    fn request<'a>(
        root: &'a Path,
        entry: &'a RuleEntry,
        harness: &'a str,
        scope: Scope,
        project_root: Option<&'a Path>,
        os_home: &'a Path,
        backups: &'a Path,
        tool_paths: &'a HashMap<String, String>,
    ) -> RuleInstallRequest<'a> {
        RuleInstallRequest {
            corpus_root: root,
            entry,
            harness,
            scope,
            project_root,
            tool_paths,
            os_home,
            backups,
            corpus_version: "baseline",
            installed_at: "2026-11-03T00:00:00Z",
        }
    }

    fn files_under(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        if !dir.exists() {
            return found;
        }
        for entry in fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                found.extend(files_under(&path));
            } else {
                found.push(path);
            }
        }
        found
    }

    // ---------- install: byte equality + ledger shape ----------

    #[tokio::test]
    async fn install_writes_the_conformance_bytes_for_every_harness() {
        let cases = [
            ("antigravity", "antigravity/typescript.md"),
            ("pi-agent", "pi-agent/typescript/RULES.md"),
            ("oh-my-pi", "oh-my-pi/typescript.md"),
            ("opencode", "opencode/typescript.md"),
        ];

        for (harness, artifact) in cases {
            let app_data = TempDir::new().expect("app data");
            let home = TempDir::new().expect("home");
            let backups = TempDir::new().expect("backups");
            let root = corpus_with(app_data.path(), &["typescript.md"]);
            let entry = entry(&root, "typescript");
            let tool_paths = empty_paths();

            let record = install_rule(request(
                &root,
                &entry,
                harness,
                Scope::User,
                None,
                home.path(),
                backups.path(),
                &tool_paths,
            ))
            .await
            .unwrap_or_else(|e| panic!("{harness}: {e:?}"));

            let expected = fixture(artifact);
            let written = fs::read(&record.dest)
                .unwrap_or_else(|e| panic!("{harness}: read {}: {e}", record.dest));
            assert_eq!(
                written, expected,
                "{harness}: installed bytes must match the conformance fixture"
            );
            assert_eq!(
                record.rendered_hash,
                sha256_hex(&expected),
                "{harness}: the ledger row must record the hash of the bytes written"
            );
            assert_eq!(
                record.rendered_hash,
                sha256_hex(&written),
                "{harness}: row and file must agree"
            );
            assert_eq!(record.domain, InstallDomain::Rule);
            assert_eq!(record.tool, harness);
            assert_eq!(record.slug, "typescript");
            assert_eq!(record.scope, Scope::User);
            assert_eq!(record.dests, vec![record.dest.clone()]);
            assert_eq!(record.source_hash, entry.source_hash);
            assert_eq!(record.body_hash, entry.body_hash);

            // The ledger serialization carries the hash the file actually has.
            let wire = serde_json::to_value(&record).expect("serialize row");
            assert_eq!(wire["domain"], "rule");
            assert_eq!(wire["renderedHash"], record.rendered_hash);
        }
    }

    #[tokio::test]
    async fn a_blocked_multiline_rule_writes_nothing_and_returns_no_row() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = rules_corpus::rules_corpus_dir(app_data.path());
        let dir = root.join("rules/base");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(
            dir.join("multi.md"),
            "---\nname: multi\ndescription: |\n  first line\n  second line\n---\n\nBody.\n",
        )
        .expect("write rule");

        let entry = entry(&root, "multi");
        assert!(entry.validation.valid, "the rule is schema-valid");
        let tool_paths = empty_paths();

        let error = install_rule(request(
            &root,
            &entry,
            "antigravity",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect_err("C11: the emitter cannot reproduce multi-line values");

        match error {
            AppError::InvalidArgument { message } => {
                assert!(message.contains("spans multiple lines"), "{message}");
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
        assert!(
            files_under(home.path()).is_empty(),
            "a refused install must write zero files: {:?}",
            files_under(home.path())
        );

        // Preview must refuse the same rule: showing bytes that install would
        // never write would be a lie (ruling C11).
        let preview = preview_rule(&root, &entry, Scope::User, None, &tool_paths, home.path());
        assert!(
            matches!(preview, Err(AppError::InvalidArgument { .. })),
            "{preview:?}"
        );
        assert!(files_under(home.path()).is_empty());
    }

    #[tokio::test]
    async fn an_invalid_rule_is_refused_before_any_write() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = rules_corpus::rules_corpus_dir(app_data.path());
        let dir = root.join("rules/base");
        fs::create_dir_all(&dir).expect("create dir");
        // Unknown key → additionalProperties: false.
        fs::write(
            dir.join("bad.md"),
            "---\nname: bad\ndescription: A bad rule.\nalwayApply: true\n---\n\nBody.\n",
        )
        .expect("write rule");

        let entry = entry(&root, "bad");
        assert!(!entry.validation.valid);
        let tool_paths = empty_paths();

        let error = install_rule(request(
            &root,
            &entry,
            "antigravity",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect_err("invalid rules never install");

        match error {
            AppError::InvalidArgument { message } => {
                assert!(
                    !message.is_empty(),
                    "the schema message is surfaced verbatim"
                );
                assert!(
                    message.contains("alwayApply") || message.contains("additional"),
                    "{message}"
                );
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
        assert!(files_under(home.path()).is_empty());
    }

    // ---------- install: OpenCode plugin side effect ----------

    #[tokio::test]
    async fn opencode_install_registers_the_plugin_and_records_it() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let record = install_rule(request(
            &root,
            &entry,
            "opencode",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect("install opencode rule");

        assert_eq!(
            record.plugin_registered,
            Some(true),
            "the first install adds the plugin entry"
        );
        let config = home.path().join(".config/opencode/opencode.json");
        let text = fs::read_to_string(&config).expect("opencode.json exists");
        assert!(
            text.contains(opencode::contract_entry().expect("contract entry")),
            "{text}"
        );

        // Re-installing is idempotent: the entry is already there, so nothing is
        // written and the row records that truthfully.
        let before = fs::read(&config).expect("read config");
        let again = install_rule(request(
            &root,
            &entry,
            "opencode",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect("reinstall opencode rule");
        assert_eq!(again.plugin_registered, Some(false));
        assert_eq!(fs::read(&config).expect("read config"), before);
    }

    #[tokio::test]
    async fn non_opencode_harnesses_never_touch_the_opencode_config() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let config = home.path().join(".config/opencode/opencode.json");
        fs::create_dir_all(config.parent().expect("parent")).expect("create config dir");
        let sentinel = b"{\n  \"model\": \"a/b\"\n}\n";
        fs::write(&config, sentinel).expect("seed config");

        for harness in ["antigravity", "pi-agent", "oh-my-pi"] {
            let record = install_rule(request(
                &root,
                &entry,
                harness,
                Scope::User,
                None,
                home.path(),
                backups.path(),
                &tool_paths,
            ))
            .await
            .unwrap_or_else(|e| panic!("{harness}: {e:?}"));
            assert_eq!(record.plugin_registered, None);
            assert_eq!(
                fs::read(&config).expect("read config"),
                sentinel,
                "{harness} must not write opencode.json"
            );
        }
    }

    // ---------- install: atomicity + project scope ----------

    #[tokio::test]
    async fn a_write_failure_leaves_no_partial_file() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        // Make the destination's parent impossible to create: a regular file
        // where the `rules/` directory must be. `create_dir_all` fails before
        // any write, and the failure is independent of filesystem permissions
        // (which a root-owned CI container would not enforce).
        let blocker = home.path().join(".gemini/config/rules");
        fs::create_dir_all(blocker.parent().expect("parent")).expect("create parent");
        fs::write(&blocker, b"not a directory").expect("seed blocker");

        let error = install_rule(request(
            &root,
            &entry,
            "antigravity",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect_err("a write failure must return an error");

        assert!(
            matches!(error, AppError::Io { .. } | AppError::Internal { .. }),
            "{error:?}"
        );
        assert!(
            !home
                .path()
                .join(".gemini/config/rules/typescript.md")
                .exists(),
            "no partial rule file"
        );
        assert!(
            files_under(&home.path().join(".gemini"))
                .iter()
                .all(|path| path == &blocker),
            "no temp sibling survived the failed write"
        );
    }

    #[tokio::test]
    async fn project_scope_without_a_root_is_refused_before_any_write() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let error = install_rule(request(
            &root,
            &entry,
            "antigravity",
            Scope::Project,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect_err("project scope needs a root");
        assert!(
            matches!(error, AppError::InvalidArgument { .. }),
            "{error:?}"
        );
        assert!(files_under(home.path()).is_empty());
    }

    #[tokio::test]
    async fn project_scope_writes_under_the_picked_root() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let project = TempDir::new().expect("project");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let record = install_rule(request(
            &root,
            &entry,
            "pi-agent",
            Scope::Project,
            Some(project.path()),
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect("project install");

        assert_eq!(record.scope, Scope::Project);
        assert_eq!(
            record.project_path.as_deref(),
            Some(project.path().to_string_lossy().as_ref())
        );
        let written = Path::new(&record.dest);
        assert!(written.starts_with(project.path()), "{written:?}");
        assert!(
            written.ends_with(".pi/rules/typescript/RULES.md"),
            "{written:?}"
        );
        assert_eq!(
            fs::read(written).expect("read"),
            fixture("pi-agent/typescript/RULES.md")
        );
        assert!(files_under(home.path()).is_empty(), "no user-scope writes");
    }

    // ---------- install: hostile slug, reinstall, backup ----------

    #[tokio::test]
    async fn a_hostile_slug_is_refused_at_the_write_boundary() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let mut hostile = entry(&root, "typescript");
        // Bypass the parser on purpose: this is the worst case where a corrupted
        // index (or a future code path) hands the writer an unvetted slug.
        hostile.slug = "../../etc/passwd".to_string();
        hostile.validation = crate::types::RuleValidation::passed();
        let tool_paths = empty_paths();

        let error = install_rule(request(
            &root,
            &hostile,
            "antigravity",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect_err("DENY-03: the slug gate runs before any join");
        match error {
            AppError::InvalidArgument { message } => {
                assert!(message.starts_with("invalid rule slug:"), "{message}");
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
        assert!(
            files_under(home.path()).is_empty(),
            "nothing escaped the root"
        );
    }

    #[tokio::test]
    async fn reinstalling_the_same_key_replaces_the_ledger_row() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let mut ledger = Vec::new();
        for _ in 0..2 {
            let record = install_rule(request(
                &root,
                &entry,
                "antigravity",
                Scope::User,
                None,
                home.path(),
                backups.path(),
                &tool_paths,
            ))
            .await
            .expect("install");
            install::upsert_rule_row(&mut ledger, record);
        }
        assert_eq!(ledger.len(), 1, "one row per (slug, harness, project)");
        assert_eq!(ledger[0].domain, InstallDomain::Rule);
        let written = fs::read(&ledger[0].dest).expect("read rule");
        assert_eq!(ledger[0].rendered_hash, sha256_hex(&written));
    }

    #[tokio::test]
    async fn a_divergent_preexisting_file_is_backed_up_before_overwrite() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let dest = home.path().join(".gemini/config/rules/typescript.md");
        fs::create_dir_all(dest.parent().expect("parent")).expect("create parent");
        fs::write(&dest, b"the user's older version\n").expect("seed dest");

        install_rule(request(
            &root,
            &entry,
            "antigravity",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect("install");

        let backup = files_under(backups.path())
            .into_iter()
            .find(|path| path.to_string_lossy().ends_with(".bak"))
            .expect("a divergent file is preserved");
        assert_eq!(
            fs::read(&backup).expect("read backup"),
            b"the user's older version\n"
        );
        assert_eq!(
            fs::read(&dest).expect("read dest"),
            fixture("antigravity/typescript.md")
        );
    }

    // ---------- preview ----------

    #[tokio::test]
    async fn preview_writes_nothing_and_reports_destinations_and_scope() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let project = TempDir::new().expect("project");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let preview = preview_rule(&root, &entry, Scope::User, None, &tool_paths, home.path())
            .expect("user preview");
        assert_eq!(preview.scope, Scope::User);
        assert_eq!(preview.project_path, None);
        assert_eq!(
            preview.harnesses.len(),
            4,
            "one entry per installable harness"
        );
        for entry in &preview.harnesses {
            assert!(entry.installable);
            assert!(!entry.bytes.is_empty(), "{} has bytes", entry.harness);
            assert_eq!(entry.rendered_hash, sha256_hex(entry.bytes.as_bytes()));
            assert!(
                entry
                    .dest
                    .starts_with(home.path().to_string_lossy().as_ref()),
                "{}: {}",
                entry.harness,
                entry.dest
            );
        }
        assert!(
            files_under(home.path()).is_empty(),
            "preview must not write anything"
        );

        let project_preview = preview_rule(
            &root,
            &entry,
            Scope::Project,
            Some(project.path()),
            &tool_paths,
            home.path(),
        )
        .expect("project preview");
        assert_eq!(project_preview.scope, Scope::Project);
        for entry in &project_preview.harnesses {
            assert!(
                entry
                    .dest
                    .starts_with(project.path().to_string_lossy().as_ref()),
                "{}: {}",
                entry.harness,
                entry.dest
            );
            assert_eq!(
                entry.bytes,
                preview
                    .harnesses
                    .iter()
                    .find(|candidate| candidate.harness == entry.harness)
                    .expect("same harness")
                    .bytes,
                "scope must not change the compiled bytes"
            );
        }
        assert!(files_under(project.path()).is_empty());
    }

    #[tokio::test]
    async fn preview_bytes_equal_installed_bytes() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let preview = preview_rule(&root, &entry, Scope::User, None, &tool_paths, home.path())
            .expect("preview");

        for harness in ["antigravity", "opencode"] {
            let previewed = preview
                .harnesses
                .iter()
                .find(|candidate| candidate.harness == harness)
                .expect("harness previewed");
            let record = install_rule(request(
                &root,
                &entry,
                harness,
                Scope::User,
                None,
                home.path(),
                backups.path(),
                &tool_paths,
            ))
            .await
            .expect("install");
            assert_eq!(
                fs::read(&record.dest).expect("read installed"),
                previewed.bytes.as_bytes(),
                "{harness}: preview and install must agree byte for byte"
            );
            assert_eq!(record.rendered_hash, previewed.rendered_hash);
            assert_eq!(record.dest, previewed.dest);
        }
    }

    #[tokio::test]
    async fn preview_refuses_an_invalid_rule_with_its_verbatim_errors() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let root = rules_corpus::rules_corpus_dir(app_data.path());
        let dir = root.join("rules/base");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(
            dir.join("bad.md"),
            "---\nname: Bad_Name\ndescription: A bad rule.\n---\n\nBody.\n",
        )
        .expect("write rule");

        let entry = entry(&root, "Bad_Name");
        assert!(!entry.validation.valid);
        let tool_paths = empty_paths();

        let error = preview_rule(&root, &entry, Scope::User, None, &tool_paths, home.path())
            .expect_err("invalid rules have no preview");
        match error {
            AppError::InvalidArgument { message } => {
                assert_eq!(message, entry.validation.errors.join("\n"));
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
        assert!(files_under(home.path()).is_empty());
    }

    #[tokio::test]
    async fn project_preview_without_a_root_is_refused() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let error = preview_rule(
            &root,
            &entry,
            Scope::Project,
            None,
            &tool_paths,
            home.path(),
        )
        .expect_err("project preview needs a root");
        assert!(matches!(error, AppError::InvalidArgument { .. }));
        assert!(files_under(home.path()).is_empty());
    }

    // ---------- get: slug ≠ filename ----------

    #[test]
    fn get_resolves_a_rule_whose_filename_differs_from_its_slug() {
        let app_data = TempDir::new().expect("app data");
        let root = corpus_with(app_data.path(), &["commit-convention.md"]);

        let index = rules_corpus::build_rules_index(&root).expect("index");
        let entry = require_entry(&index, "commit-conventions").expect("slug from frontmatter");
        assert_eq!(entry.relative_path, "rules/base/commit-convention.md");
        assert!(
            !index.contains_key("commit-convention"),
            "the filename is not a slug"
        );

        let rule = read_rule(&root, entry).expect("read through relative_path");
        assert_eq!(rule.slug, "commit-conventions");
        assert_eq!(rule.relative_path, "rules/base/commit-convention.md");
        assert!(!rule.body.is_empty());
        assert_eq!(
            sha256_hex(rule.body.as_bytes()),
            rule.body_hash,
            "body hash matches the normalized body"
        );
    }

    #[test]
    fn unknown_slug_names_the_slug() {
        let index = BTreeMap::new();
        match require_entry(&index, "nope") {
            Err(AppError::InvalidArgument { message }) => assert_eq!(message, "unknown rule: nope"),
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    // ---------- list ----------

    #[test]
    fn list_omits_bodies_orders_deterministically_and_keeps_invalid_rows() {
        let app_data = TempDir::new().expect("app data");
        let root = rules_corpus::rules_corpus_dir(app_data.path());
        fs::create_dir_all(root.join("rules/stacks")).expect("create stacks");
        fs::create_dir_all(root.join("rules/base")).expect("create base");
        fs::write(
            root.join("rules/stacks/typescript.md"),
            fixture("sources/typescript.md"),
        )
        .expect("write typescript");
        fs::write(
            root.join("rules/base/agentmemory.md"),
            fixture("sources/agentmemory.md"),
        )
        .expect("write agentmemory");
        fs::write(
            root.join("rules/base/bad.md"),
            "---\nname: bad\ndescription: A bad rule.\nalwayApply: true\n---\n\nBody.\n",
        )
        .expect("write bad");

        let index = rules_corpus::build_rules_index(&root).expect("index");
        let rows = list_summaries(&index, None);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().map(|row| row.slug.as_str()).collect::<Vec<_>>(),
            vec!["agentmemory", "bad", "typescript"],
            "category, then name"
        );
        let bad = rows.iter().find(|row| row.slug == "bad").expect("listed");
        assert!(!bad.validation.valid);
        assert!(!bad.validation.errors.is_empty());

        // The payload cannot leak a body: `RuleSummary` has no such field.
        let wire = serde_json::to_value(&rows).expect("serialize rows");
        assert!(
            wire.as_array()
                .expect("array")
                .iter()
                .all(|row| row.get("body").is_none()),
            "{wire}"
        );

        let filtered = list_summaries(&index, Some("stacks"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].slug, "typescript");
    }

    // ---------- uninstall ----------

    #[tokio::test]
    async fn uninstall_removes_exactly_the_recorded_files_and_keeps_the_plugin() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let record = install_rule(request(
            &root,
            &entry,
            "opencode",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect("install");

        let config = home.path().join(".config/opencode/opencode.json");
        let plugin_bytes = fs::read(&config).expect("config written");
        let sentinel = home.path().join("keep-me.txt");
        fs::write(&sentinel, b"not ours").expect("seed sentinel");

        let report = remove_rule_files(&record, &empty_paths(), home.path())
            .await
            .expect("uninstall");
        assert_eq!(report.removed, vec![record.dest.clone()]);
        assert!(report.missing.is_empty());
        assert!(!Path::new(&record.dest).exists(), "rule file removed");
        assert!(sentinel.exists(), "a path not in dests is untouched");
        assert_eq!(
            fs::read(&config).expect("read config"),
            plugin_bytes,
            "the OpenCode plugin entry must never be removed"
        );

        // Idempotent: a second call reports the missing file and still succeeds.
        let again = remove_rule_files(&record, &empty_paths(), home.path())
            .await
            .expect("second uninstall");
        assert!(again.removed.is_empty());
        assert_eq!(again.missing, vec![record.dest.clone()]);
    }

    #[tokio::test]
    async fn uninstall_never_prunes_the_empty_directory_pi_agent_leaves() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let record = install_rule(request(
            &root,
            &entry,
            "pi-agent",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect("install");
        let dir = Path::new(&record.dest)
            .parent()
            .expect("nested dir")
            .to_path_buf();

        remove_rule_files(&record, &empty_paths(), home.path())
            .await
            .expect("uninstall");
        assert!(dir.is_dir(), "ruling A8: uninstall deletes files only");
        assert!(!Path::new(&record.dest).exists());
    }

    #[tokio::test]
    async fn a_relative_project_root_is_refused_before_any_write() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let error = install_rule(request(
            &root,
            &entry,
            "antigravity",
            Scope::Project,
            Some(Path::new("relative-project")),
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect_err("a relative project root must be refused");
        assert!(
            error.to_string().contains("must be absolute"),
            "unexpected error: {error}"
        );
        assert!(
            !Path::new("relative-project").exists(),
            "nothing may be created in the process CWD"
        );
    }

    #[tokio::test]
    async fn uninstall_refuses_a_ledger_row_that_names_an_unrelated_file() {
        let app_data = TempDir::new().expect("app data");
        let home = TempDir::new().expect("home");
        let backups = TempDir::new().expect("backups");
        let root = corpus_with(app_data.path(), &["typescript.md"]);
        let entry = entry(&root, "typescript");
        let tool_paths = empty_paths();

        let mut record = install_rule(request(
            &root,
            &entry,
            "antigravity",
            Scope::User,
            None,
            home.path(),
            backups.path(),
            &tool_paths,
        ))
        .await
        .expect("install");

        // Simulate a hand-edited ledger: the row now claims a file the app never
        // wrote. The recomputed destination set must reject it — and reject the
        // whole operation, so even the legitimate path survives.
        let victim = home.path().join("unrelated-user-file.txt");
        fs::write(&victim, b"do not delete me").expect("victim");
        record.dest = victim.to_string_lossy().into_owned();
        record.dests = vec![record.dest.clone()];

        let error = remove_rule_files(&record, &empty_paths(), home.path())
            .await
            .expect_err("a forged destination must be refused");
        assert!(
            error.to_string().contains("not a destination"),
            "unexpected error: {error}"
        );
        assert!(victim.exists(), "the unrelated file must survive");
    }
}
