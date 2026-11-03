//! Rules corpus — the second managed corpus (`deuriib/agency-rules`), TASK 11–12.
//!
//! ```text
//! <app_data_dir>/
//! ├── rules-corpus/               our maintained copy of the agency-rules repo
//! │   └── rules/<category>/<file>.md
//! └── state/
//!     ├── rules-index.json        slug → RuleEntry (hashes, category, validation)
//!     └── rules-meta.json         version / fetched_at / count
//! ```
//!
//! The roots are deliberately **disjoint** from the agent corpus
//! (`corpus/`, `corpus-index.json`, `corpus-meta.json`): the two repositories
//! refresh independently, and one must never be able to overwrite the other
//! (TASK 25 asserts the paths differ for the same app-data dir).
//!
//! ## Seed (offline first run)
//!
//! The baseline ships in the app bundle at
//! `<resources>/resources/rules-baseline/rules/` — a verbatim `rules/` tree,
//! including `_schema.json`. When `<app_data>/rules-corpus` is empty it is
//! recursively copied in with [`crate::corpus::copy_tree`], so a fresh install
//! lists and previews rules with no network.
//!
//! ## Refresh (no runtime git)
//!
//! [`refresh`] downloads
//! `https://codeload.github.com/deuriib/agency-rules/tar.gz/refs/heads/main`,
//! extracts the `rules/**` subtree into a staging directory, atomically swaps it
//! over `rules-corpus/`, and rebuilds the index. The resolved commit SHA is
//! recorded as the corpus version. The extraction itself is
//! [`crate::corpus::extract_archive`] — the same traversal-guarded, byte-capped
//! loop the agent corpus uses, not a second copy of it.
//!
//! ## Determinism
//!
//! `rules-index.json` is a `BTreeMap` keyed by the frontmatter `name` (the
//! slug), so its serialization has a stable key order, and it carries **no
//! timestamp** — the same contract as `corpus-index.json`
//! (`systemPatterns.md` §2). The only clock value in the subsystem lives in
//! `rules-meta.json`, which is not part of the index.
//!
//! ## Invalid rules
//!
//! A rule that parses but fails the schema gate is **retained** in the index
//! with its [`crate::types::RuleValidation`], so the UI can list it and explain
//! why install is refused (spec §10.4: a visible error, not an invisible rule). A file that
//! cannot be parsed into a rule at all — no frontmatter, malformed YAML, or no
//! `name` — cannot be keyed and is skipped with a warning; an unreadable or
//! non-UTF-8 file is an error, because that is a broken corpus rather than a
//! non-rule.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::corpus::{
    copy_tree, download_tarball, extract_archive, is_empty_dir, ArchiveKeep, ExtractedArchive,
    BASELINE_VERSION, MAX_AGENT_BYTES, USER_AGENT,
};
use crate::error::AppError;
use crate::rules::parse::{is_rule_file, parse_rule};
use crate::types::{Rule, RuleEmptyReason, RuleEntry, RuleStatus};
use crate::util::fs::atomic_write;

// ---------- Constants ----------

/// GitHub `codeload` tarball for the live rules corpus. Streamed, gunzipped and
/// unpacked by [`install_tarball`]; no git binary required.
const RULES_TARBALL_URL: &str =
    "https://codeload.github.com/deuriib/agency-rules/tar.gz/refs/heads/main";

/// The commit the tarball resolves to, read with `Accept:
/// application/vnd.github.sha` (a single short text response). Best effort: the
/// tar stream itself does not carry the SHA, so a failure here degrades the
/// recorded version to a dated ref marker rather than failing the refresh.
const RULES_HEAD_SHA_URL: &str = "https://api.github.com/repos/deuriib/agency-rules/commits/main";

/// Timeout for the tiny version probe. The tarball download keeps its own
/// (longer) [`crate::corpus`] budget.
const VERSION_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Archive entries kept by the rules extractor: the whole `rules/` subtree and
/// nothing else. No allow-list of categories inside it — a new upstream category
/// must appear without an app update (`decisions.md` 2026-11-02).
const RULES_TOP_LEVEL: &str = "rules";

// ---------- Paths ----------

/// The rules working copy: `<app_data_dir>/rules-corpus`. ALWAYS derived from
/// app data — never composed from IPC input.
pub fn rules_corpus_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("rules-corpus")
}

/// The persisted index: `<app_data_dir>/state/rules-index.json`.
pub fn rules_index_path(app_data_dir: &Path) -> PathBuf {
    crate::corpus::state_dir(app_data_dir).join("rules-index.json")
}

/// The persisted meta: `<app_data_dir>/state/rules-meta.json`. Separate from the
/// index because it is the only file carrying a timestamp.
pub fn rules_meta_path(app_data_dir: &Path) -> PathBuf {
    crate::corpus::state_dir(app_data_dir).join("rules-meta.json")
}

/// Where a refresh extracts before the atomic swap. Inside app data on purpose:
/// a rename is only atomic on the same filesystem, and nothing may be written
/// outside the app-data dir.
fn staging_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("rules-corpus.staging")
}

/// The previous working copy during a swap, removed once the swap succeeds.
fn backup_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("rules-corpus.old")
}

// ---------- On-disk meta ----------

/// `rules-meta.json` — version / freshness / count. Distinct from the index so
/// the index stays timestamp-free and byte-stable across launches.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRulesMeta {
    /// The resolved upstream commit SHA after a refresh; `"baseline"` before one.
    pub version: String,
    pub fetched_at: String,
    pub count: u32,
}

/// The persisted index envelope: `{ "count": N, "rules": { "<slug>": … } }`.
/// Borrowed, so writing the index never clones the rows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RulesIndexFile<'a> {
    count: u32,
    rules: &'a BTreeMap<String, RuleEntry>,
}

/// Serialize the index to canonical pretty JSON. Stable key order (`BTreeMap`)
/// and no timestamp ⇒ byte-identical output for an unchanged corpus.
fn index_json(index: &BTreeMap<String, RuleEntry>) -> Result<Vec<u8>, AppError> {
    let file = RulesIndexFile {
        count: index.len() as u32,
        rules: index,
    };
    serde_json::to_vec_pretty(&file).map_err(|e| AppError::Internal {
        message: format!("serialize rules-index.json: {e}"),
    })
}

// ---------- Index build ----------

/// Scan `<root>/rules/**` into `slug → RuleEntry`.
///
/// The slug is the **frontmatter `name`**, never the filename (ruling A3 /
/// C6): `rules/base/commit-convention.md` declares `name: commit-conventions`,
/// so the row also carries `relative_path` for `rules_get` to read the body.
///
/// Pure of clock and network — a function of the tree only, which is what makes
/// the byte-stability test meaningful. Errors:
///
/// - the root itself is missing/unreadable → `Io` naming the path;
/// - the root exists but has no `rules/` → `Io` naming the root;
/// - a rule file cannot be read (I/O or non-UTF-8) → `Io` naming the file;
/// - two files declare the same slug → `InvalidArgument` naming **both** paths,
///   because upstream's `validate.mjs` treats uniqueness as fatal and silently
///   deduping would drop a rule the user can see in the repo.
///
/// A schema-invalid rule is retained with its `RuleValidation`; a file that is
/// not a rule at all (no frontmatter, no `name`) is skipped with a warning.
pub fn build_rules_index(root: &Path) -> Result<BTreeMap<String, RuleEntry>, AppError> {
    let metadata = std::fs::metadata(root).map_err(|e| AppError::Io {
        message: format!("read {}: {e}", root.display()),
    })?;
    if !metadata.is_dir() {
        return Err(AppError::Io {
            message: format!("read {}: not a directory", root.display()),
        });
    }

    let rules_dir = root.join(RULES_TOP_LEVEL);
    if !rules_dir.is_dir() {
        return Err(AppError::Io {
            message: format!("{} has no rules/ directory", root.display()),
        });
    }

    let mut files = Vec::new();
    collect_rule_files(&rules_dir, &mut files)?;
    files.sort();

    let mut index: BTreeMap<String, RuleEntry> = BTreeMap::new();
    for path in files {
        let relative_path = relative_path(root, &path)?;
        // Skips `_schema.json`, `_draft.md`, and any other `_`-prefixed segment
        // — the same predicate the parser documents, so a scratch file can
        // never surface as an installable rule.
        if !is_rule_file(&relative_path) {
            continue;
        }

        let bytes = std::fs::read(&path).map_err(|e| AppError::Io {
            message: format!("read {}: {e}", path.display()),
        })?;
        // The same per-file cap the agent corpus uses: a rule is a corpus
        // source file, and one shared ceiling keeps both extractors honest.
        if bytes.len() as u64 > MAX_AGENT_BYTES {
            return Err(AppError::Io {
                message: format!("{} exceeds {} byte cap", path.display(), MAX_AGENT_BYTES),
            });
        }

        match parse_rule(&relative_path, &bytes) {
            Ok(rule) => {
                let entry = entry_from_rule(rule);
                if let Some(existing) = index.get(&entry.slug) {
                    return Err(AppError::InvalidArgument {
                        message: format!(
                            "duplicate rule slug '{}' in {} and {}",
                            entry.slug, existing.relative_path, relative_path
                        ),
                    });
                }
                index.insert(entry.slug.clone(), entry);
            }
            Err(AppError::InvalidArgument { message }) => {
                // A file under `rules/` that is not a rule (a README, a scratch
                // note) must not abort the whole catalog — there is no slug to
                // key it by, so it is reported and skipped.
                tracing::warn!("rules: skipping {}: {message}", path.display());
            }
            Err(error) => return Err(error),
        }
    }
    Ok(index)
}

/// Recursively collect `*.md` under `dir`, skipping `_`-prefixed directories.
/// Paths are sorted by the caller so the index order never depends on the OS.
fn collect_rule_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir).map_err(|e| AppError::Io {
        message: format!("read {}: {e}", dir.display()),
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| AppError::Io {
            message: format!("read {}: {e}", dir.display()),
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| AppError::Io {
            message: format!("read {}: {e}", path.display()),
        })?;
        if file_type.is_dir() {
            if entry.file_name().to_string_lossy().starts_with('_') {
                continue;
            }
            collect_rule_files(&path, out)?;
        } else if file_type.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("md")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// The corpus-relative path with `/` separators, e.g. `rules/base/x.md`.
fn relative_path(root: &Path, path: &Path) -> Result<String, AppError> {
    let relative = path.strip_prefix(root).map_err(|_| AppError::Internal {
        message: format!("{} is outside {}", path.display(), root.display()),
    })?;
    let mut segments = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => {
                let text = segment.to_str().ok_or_else(|| AppError::Io {
                    message: format!("read {}: path is not valid UTF-8", path.display()),
                })?;
                segments.push(text.to_string());
            }
            _ => {
                return Err(AppError::Io {
                    message: format!("read {}: unexpected path component", path.display()),
                })
            }
        }
    }
    Ok(segments.join("/"))
}

/// Drop the body (the index never carries it — `corpus_list`'s contract) and
/// keep every field a list row or a renderer needs.
fn entry_from_rule(rule: Rule) -> RuleEntry {
    RuleEntry {
        slug: rule.slug,
        name: rule.name,
        description: rule.description,
        category: rule.category,
        relative_path: rule.relative_path,
        globs: rule.globs,
        always_apply: rule.always_apply,
        trigger: rule.trigger,
        match_: rule.match_,
        tags: rule.tags,
        tools: rule.tools,
        source_hash: rule.source_hash,
        body_hash: rule.body_hash,
        validation: rule.validation,
    }
}

// ---------- Seed ----------

/// Seed `rules-corpus/` from the bundled baseline when it is empty.
///
/// Idempotent: a populated root is left untouched, so a user's refreshed corpus
/// is never stamped back to the bundled baseline. A seed writes the index and
/// meta too, so the first read command has a usable index with no network.
pub async fn seed_if_empty(app_data_dir: &Path, baseline_dir: &Path) -> Result<(), AppError> {
    let root = rules_corpus_dir(app_data_dir);
    if !is_empty_dir(&root) {
        return Ok(());
    }

    let source = baseline_dir.join(RULES_TOP_LEVEL);
    if !source.is_dir() {
        return Err(AppError::Io {
            message: format!(
                "rules baseline not found at {} (expected {})",
                baseline_dir.display(),
                source.display()
            ),
        });
    }

    let copied = copy_tree(&source, &root.join(RULES_TOP_LEVEL), 0);
    if copied == 0 {
        return Err(AppError::Io {
            message: format!("rules baseline at {} contained no files", source.display()),
        });
    }
    tracing::info!("rules: seeded {copied} files from the bundled baseline");

    // A seeded corpus must be usable with no network, i.e. the index has to
    // exist on disk before the first read command returns.
    let index = build_rules_index(&root)?;
    write_index(app_data_dir, &index).await?;
    write_meta(
        app_data_dir,
        &StoredRulesMeta {
            version: BASELINE_VERSION.to_string(),
            fetched_at: now_rfc3339(),
            count: index.len() as u32,
        },
    )
    .await?;
    Ok(())
}

// ---------- Status ----------

/// `rules_status` — seed (first run only), then report the synced state.
///
/// The distinction spec §8 requires lives here: `emptyReason = "notSynced"` when
/// there is no usable corpus root, `"noRules"` when a real root scans to zero
/// rules, and `null` when there is something to show.
///
/// A seed failure is logged and reported as `notSynced` rather than aborting the
/// section: the honest state is "nothing is synced", and the refresh CTA can
/// repair it (same posture as `corpus::resolve_active`).
pub async fn status(app_data_dir: &Path, baseline_dir: &Path) -> Result<RuleStatus, AppError> {
    if let Err(error) = seed_if_empty(app_data_dir, baseline_dir).await {
        tracing::warn!("rules: seed from baseline failed: {error}");
    }
    read_status(app_data_dir).await
}

/// Report the corpus state from disk, without seeding. Split out so a caller
/// that has already seeded (or deliberately must not) reads the same truth.
pub async fn read_status(app_data_dir: &Path) -> Result<RuleStatus, AppError> {
    let root = rules_corpus_dir(app_data_dir);
    let stored = load_meta(app_data_dir).await?;
    let version = stored
        .map(|meta| meta.version)
        .unwrap_or_else(|| BASELINE_VERSION.to_string());

    // "Configured" means there is a rules tree to read, not merely a directory
    // that exists: a root left with only debris is not a synced corpus.
    let configured = root.join(RULES_TOP_LEVEL).is_dir() && !is_empty_dir(&root);
    if !configured {
        return Ok(RuleStatus {
            configured: false,
            count: 0,
            version,
            empty_reason: Some(RuleEmptyReason::NotSynced),
        });
    }

    let index = build_rules_index(&root)?;
    let count = index.len() as u32;
    Ok(RuleStatus {
        configured: true,
        count,
        version,
        empty_reason: if count == 0 {
            Some(RuleEmptyReason::NoRules)
        } else {
            None
        },
    })
}

// ---------- Refresh ----------

/// Fetch the upstream tarball, swap it in, rebuild the index, and report the
/// fresh status.
///
/// Caller-side gates (Offline Mode, single-flight) live in
/// `rules::commands::rules_refresh`: this function assumes it is allowed to
/// reach the network.
pub async fn refresh(app_data_dir: &Path) -> Result<RuleStatus, AppError> {
    let bytes = download_tarball(RULES_TARBALL_URL).await?;
    let version = resolve_version().await;
    install_tarball(app_data_dir, &bytes, &version).await?;
    read_status(app_data_dir).await
}

/// Extract a rules tarball into the working copy, atomically, and rebuild the
/// persisted index. Returns the number of rule files written.
///
/// Split from [`refresh`] so the whole swap-and-reindex path is provable against
/// a local fixture tarball, with no network (the QA gate for TASK 12).
pub async fn install_tarball(
    app_data_dir: &Path,
    tar_gz: &[u8],
    version: &str,
) -> Result<u32, AppError> {
    let staging = staging_dir(app_data_dir);
    if staging.exists() {
        // Crash residue from an earlier refresh: never extract on top of it.
        std::fs::remove_dir_all(&staging).map_err(|e| AppError::Io {
            message: format!("remove stale {}: {e}", staging.display()),
        })?;
    }
    std::fs::create_dir_all(&staging).map_err(|e| AppError::Io {
        message: format!("create {}: {e}", staging.display()),
    })?;

    // Prove the archive can become a rules corpus BEFORE replacing a working
    // one: a tarball with no rules, or with a duplicate slug, must leave the
    // current corpus untouched and clean its staging dir up.
    let prepared = prepare_staging(&staging, tar_gz);
    let (index, extracted) = match prepared {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
    };

    let root = rules_corpus_dir(app_data_dir);

    // F1 — a corpus that parses to zero rules is not an upgrade. `primary`
    // counts files that *look* like rules; the index only keeps ones that can
    // be keyed. An archive of malformed `.md` files would otherwise replace a
    // healthy corpus with an empty one.
    if index.is_empty() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(AppError::Internal {
            message: "rules tarball produced no indexable rules under rules/".into(),
        });
    }

    // F2 — prove the state files can be written BEFORE the swap. Writing them
    // afterwards meant a permission/space failure returned `Err` with the new
    // corpus already live and the old version string still on disk.
    let state = crate::corpus::state_dir(app_data_dir);
    let index_pending = state.join("rules-index.json.pending");
    let meta_pending = state.join("rules-meta.json.pending");
    let meta = StoredRulesMeta {
        version: version.to_string(),
        fetched_at: now_rfc3339(),
        count: index.len() as u32,
    };
    let index_bytes = index_json(&index)?;
    let meta_bytes = serde_json::to_vec_pretty(&meta).map_err(|e| AppError::Internal {
        message: format!("serialize rules-meta.json: {e}"),
    })?;
    let prepared = async {
        tokio::fs::create_dir_all(&state)
            .await
            .map_err(|e| AppError::Io {
                message: format!("create state dir {}: {e}", state.display()),
            })?;
        tokio::fs::write(&index_pending, &index_bytes)
            .await
            .map_err(|e| AppError::Io {
                message: format!("write {}: {e}", index_pending.display()),
            })?;
        tokio::fs::write(&meta_pending, &meta_bytes)
            .await
            .map_err(|e| AppError::Io {
                message: format!("write {}: {e}", meta_pending.display()),
            })
    }
    .await;
    if let Err(error) = prepared {
        let _ = std::fs::remove_file(&index_pending);
        let _ = std::fs::remove_file(&meta_pending);
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    swap_dir(&staging, &root, &backup_dir(app_data_dir))?;
    tracing::info!(
        "rules: refreshed {} files ({} rules) into {}",
        extracted.written.len(),
        extracted.primary,
        root.display()
    );

    // Two same-filesystem renames: the bytes are already on disk, so this
    // window is measured in microseconds and a failure leaves a stale index
    // that `status` rebuilds from the tree rather than a lie.
    tokio::fs::rename(&index_pending, rules_index_path(app_data_dir))
        .await
        .map_err(|e| AppError::Io {
            message: format!("commit rules-index.json: {e}"),
        })?;
    tokio::fs::rename(&meta_pending, rules_meta_path(app_data_dir))
        .await
        .map_err(|e| AppError::Io {
            message: format!("commit rules-meta.json: {e}"),
        })?;
    Ok(extracted.primary)
}

/// Extract the tarball into `staging` and index the result, without touching the
/// live corpus.
///
/// `rules/**` verbatim — `_schema.json`, nested categories, future
/// subdirectories — and nothing outside it. A `.md` the index will accept counts
/// as a rule file; the rest (`_schema.json`, `_`-prefixed scratch files) is
/// carried along without inflating the count.
fn prepare_staging(
    staging: &Path,
    tar_gz: &[u8],
) -> Result<(BTreeMap<String, RuleEntry>, ExtractedArchive), AppError> {
    let extracted = extract_archive(tar_gz, staging, |components| {
        if components[0] != RULES_TOP_LEVEL {
            return ArchiveKeep::Skip;
        }
        if is_rule_file(&components.join("/")) {
            ArchiveKeep::Primary
        } else {
            ArchiveKeep::Write
        }
    })?;
    if extracted.primary == 0 {
        return Err(AppError::Internal {
            message: "rules tarball contained no rule files under rules/".into(),
        });
    }
    let index = build_rules_index(staging)?;
    Ok((index, extracted))
}

/// Replace `target` with `staging`, restoring the previous tree if the second
/// rename fails.
///
/// Two renames are not one atomic operation, so the failure window is closed by
/// keeping the old tree at `backup` until the new one is in place. A reader in
/// that window sees no corpus at all rather than a half-written one, which the
/// next launch repairs by seeding.
fn swap_dir(staging: &Path, target: &Path, backup: &Path) -> Result<(), AppError> {
    if backup.exists() {
        std::fs::remove_dir_all(backup).map_err(|e| AppError::Io {
            message: format!("remove stale {}: {e}", backup.display()),
        })?;
    }

    let had_target = target.exists();
    if had_target {
        std::fs::rename(target, backup).map_err(|e| AppError::Io {
            message: format!("swap {} -> {}: {e}", target.display(), backup.display()),
        })?;
    }

    if let Err(e) = std::fs::rename(staging, target) {
        if had_target {
            let _ = std::fs::rename(backup, target);
        }
        return Err(AppError::Io {
            message: format!("swap {} -> {}: {e}", staging.display(), target.display()),
        });
    }

    if had_target {
        // The swap already succeeded; a leftover backup only costs disk.
        if let Err(e) = std::fs::remove_dir_all(backup) {
            tracing::warn!("rules: could not remove {}: {e}", backup.display());
        }
    }
    Ok(())
}

/// Resolve the commit the tarball corresponds to.
///
/// Best effort by design: the codeload stream carries no SHA, and failing the
/// whole refresh because a supplementary API call was rate-limited would trade
/// working content for a prettier version string. A failure is logged and the
/// version degrades to a dated `github:main@…` marker (the same shape the agent
/// corpus records when git is unavailable).
async fn resolve_version() -> String {
    match probe_head_sha().await {
        Ok(sha) => sha,
        Err(error) => {
            tracing::warn!(
                "rules: could not resolve the upstream commit sha ({error}); recording a ref marker"
            );
            format!("github:main@{}", chrono::Utc::now().format("%Y-%m-%d"))
        }
    }
}

/// `GET /repos/{owner}/{repo}/commits/main` with `Accept:
/// application/vnd.github.sha`, which returns the bare SHA as text.
async fn probe_head_sha() -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .timeout(VERSION_PROBE_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| AppError::Network {
            url: RULES_HEAD_SHA_URL.to_string(),
            message: format!("client build: {e}"),
        })?;
    let response = client
        .get(RULES_HEAD_SHA_URL)
        .header(reqwest::header::ACCEPT, "application/vnd.github.sha")
        .send()
        .await
        .map_err(|e| AppError::Network {
            url: RULES_HEAD_SHA_URL.to_string(),
            message: e.to_string(),
        })?;
    if !response.status().is_success() {
        return Err(AppError::HttpStatus {
            url: RULES_HEAD_SHA_URL.to_string(),
            status: response.status().as_u16(),
        });
    }
    let body = response.text().await.map_err(|e| AppError::Network {
        url: RULES_HEAD_SHA_URL.to_string(),
        message: format!("read body: {e}"),
    })?;
    let sha = body.trim();
    if sha.len() >= 7 && sha.len() <= 64 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(sha.to_string())
    } else {
        Err(AppError::Internal {
            message: format!("unexpected commit response from {RULES_HEAD_SHA_URL}: {sha:?}"),
        })
    }
}

// ---------- Persistence ----------

async fn write_index(
    app_data_dir: &Path,
    index: &BTreeMap<String, RuleEntry>,
) -> Result<(), AppError> {
    let state = crate::corpus::state_dir(app_data_dir);
    tokio::fs::create_dir_all(&state)
        .await
        .map_err(|e| AppError::Io {
            message: format!("create state dir {}: {e}", state.display()),
        })?;
    let bytes = index_json(index)?;
    atomic_write(&rules_index_path(app_data_dir), &bytes).await
}

async fn write_meta(app_data_dir: &Path, meta: &StoredRulesMeta) -> Result<(), AppError> {
    let state = crate::corpus::state_dir(app_data_dir);
    tokio::fs::create_dir_all(&state)
        .await
        .map_err(|e| AppError::Io {
            message: format!("create state dir {}: {e}", state.display()),
        })?;
    let bytes = serde_json::to_vec_pretty(meta).map_err(|e| AppError::Internal {
        message: format!("serialize rules-meta.json: {e}"),
    })?;
    atomic_write(&rules_meta_path(app_data_dir), &bytes).await
}

/// Load `rules-meta.json`. `None` only when the file is absent — every other
/// failure carries the path and the reason, mirroring `read_runbooks`.
async fn load_meta(app_data_dir: &Path) -> Result<Option<StoredRulesMeta>, AppError> {
    let path = rules_meta_path(app_data_dir);
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(AppError::Io {
                message: format!("read {}: {e}", path.display()),
            })
        }
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| AppError::Io {
            message: format!("parse {}: {e}", path.display()),
        })
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RuleEmptyReason;

    const RULE_X: &str = "---\nname: x\ndescription: A test rule.\n---\n\n# X\n\n- do the thing\n";

    /// Write a rule whose frontmatter `name`, and therefore slug, is `name`.
    /// The filename is independent — the corpus proves slug ≠ filename.
    fn write_rule(root: &Path, relative: &str, name: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("rule path has a parent"))
            .expect("create category dir");
        std::fs::write(
            &path,
            format!("---\nname: {name}\ndescription: A test rule.\n---\n\n# {name}\n\n- body\n"),
        )
        .expect("write rule");
    }

    fn write_rule_with_frontmatter(root: &Path, relative: &str, frontmatter: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("rule path has a parent"))
            .expect("create category dir");
        std::fs::write(&path, format!("---\n{frontmatter}\n---\n\n## Body\n")).expect("write rule");
    }

    /// Build a `.tar.gz` in the codeload layout for `agency-rules`
    /// (`<top>/<path>`). Local to these tests: the agent corpus's helper wraps
    /// its own repo name, and a shared one would hide which repo a fixture
    /// simulates.
    fn tarball(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        {
            let mut builder = tar::Builder::new(&mut gz);
            for (path, body) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(
                        &mut header,
                        format!("agency-rules-main/{path}"),
                        body.as_bytes(),
                    )
                    .expect("append tar entry");
            }
            builder.finish().expect("finish tarball");
        }
        gz.finish().expect("finish gzip")
    }

    /// Build a tarball whose entry names are written byte-for-byte into the tar
    /// headers, bypassing the `tar` crate's own path validation. That is the only
    /// way to simulate a hostile archive: the crate refuses to *create* a `..`
    /// entry, but a malicious server can hand one to us.
    fn raw_named_tarball(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        {
            let mut builder = tar::Builder::new(&mut gz);
            for (name, body) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_entry_type(tar::EntryType::Regular);
                {
                    let old = header.as_old_mut();
                    assert!(
                        name.len() <= old.name.len(),
                        "name too long for a ustar header"
                    );
                    old.name[..name.len()].copy_from_slice(name.as_bytes());
                }
                header.set_cksum();
                builder
                    .append(&header, body.as_bytes())
                    .expect("append raw-named entry");
            }
            builder.finish().expect("finish tarball");
        }
        gz.finish().expect("finish gzip")
    }

    // ── index build ─────────────────────────────────────────────────────────

    #[test]
    fn index_skips_underscore_entries_and_non_markdown() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_rule(root, "rules/base/ok.md", "ok");

        // The real corpus ships this file at the rules root.
        std::fs::write(root.join("rules/_schema.json"), "{}").expect("write schema");
        std::fs::write(root.join("rules/base/notes.txt"), "not a rule").expect("write notes");
        write_rule(root, "rules/_draft/wip.md", "wip");

        let index = build_rules_index(root).expect("index builds");
        assert_eq!(
            index.keys().collect::<Vec<_>>(),
            vec!["ok"],
            "_schema.json, non-.md files and _-prefixed dirs must never be rules"
        );
    }

    #[test]
    fn index_nests_by_category() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_rule(root, "rules/base/one.md", "one");
        write_rule(root, "rules/stacks/two.md", "two");

        let index = build_rules_index(root).expect("index builds");
        assert_eq!(index["one"].category, "base");
        assert_eq!(index["two"].category, "stacks");
        assert_eq!(index["one"].relative_path, "rules/base/one.md");
        assert_eq!(index["two"].relative_path, "rules/stacks/two.md");
    }

    /// C6 / A3: the key and the filename disagree in the real corpus.
    #[test]
    fn index_keys_by_frontmatter_slug_and_keeps_the_source_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_rule_with_frontmatter(
            root,
            "rules/base/commit-convention.md",
            "name: commit-conventions\ndescription: Conventional Commits.",
        );

        let index = build_rules_index(root).expect("index builds");
        assert!(index.contains_key("commit-conventions"));
        assert!(
            !index.contains_key("commit-convention"),
            "the filename must never become the identity"
        );
        let entry = &index["commit-conventions"];
        assert_eq!(entry.slug, entry.name);
        assert_eq!(entry.relative_path, "rules/base/commit-convention.md");
        assert!(entry.validation.valid, "{:?}", entry.validation.errors);
    }

    #[test]
    fn duplicate_slug_across_categories_names_both_paths() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_rule(root, "rules/alpha/dup.md", "dup");
        write_rule(root, "rules/beta/dup-again.md", "dup");

        let error = build_rules_index(root).expect_err("duplicates must be fatal");
        let AppError::InvalidArgument { message } = error else {
            panic!("expected InvalidArgument, got {error:?}");
        };
        assert!(message.contains("duplicate rule slug 'dup'"), "{message}");
        assert!(message.contains("rules/alpha/dup.md"), "{message}");
        assert!(message.contains("rules/beta/dup-again.md"), "{message}");
    }

    /// Spec §10.4: a visible error, not an invisible rule.
    #[test]
    fn an_invalid_rule_is_retained_with_its_validation() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_rule_with_frontmatter(
            root,
            "rules/base/bad.md",
            "name: Bad Name\ndescription: Uppercase name fails the pattern.",
        );

        let index = build_rules_index(root).expect("an invalid rule never aborts the index");
        let entry = &index["Bad Name"];
        assert!(!entry.validation.valid);
        assert!(!entry.validation.errors.is_empty());
        assert!(
            entry
                .validation
                .errors
                .iter()
                .any(|error| error.contains("does not match")),
            "{:?}",
            entry.validation.errors
        );
    }

    #[test]
    fn index_carries_no_timestamps_and_is_byte_stable() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        write_rule(root, "rules/base/one.md", "one");
        write_rule(root, "rules/stacks/two.md", "two");

        let first = build_rules_index(root).expect("first build");
        let second = build_rules_index(root).expect("second build");
        assert_eq!(
            index_json(&first).expect("serialize"),
            index_json(&second).expect("serialize"),
            "two builds over the same tree must serialize byte-identically"
        );

        let text = String::from_utf8(index_json(&first).expect("serialize")).expect("utf-8");
        for forbidden in [
            "fetchedAt",
            "installedAt",
            "updatedAt",
            "createdAt",
            "timestamp",
            "Timestamp",
        ] {
            assert!(
                !text.contains(forbidden),
                "index must not carry {forbidden}"
            );
        }
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        let mut keys: Vec<&str> = parsed
            .as_object()
            .expect("envelope is an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["count", "rules"]);
        assert_eq!(parsed["count"], 2);
    }

    #[test]
    fn a_missing_root_is_an_io_error_naming_the_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("absent");
        let error = build_rules_index(&root).expect_err("missing root must fail");
        match error {
            AppError::Io { message } => assert!(message.contains("absent"), "{message}"),
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn a_root_without_a_rules_dir_is_its_own_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let error = build_rules_index(tmp.path()).expect_err("root without rules/ must fail");
        match error {
            AppError::Io { message } => {
                assert!(message.contains("has no rules/ directory"), "{message}")
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }

    /// A rule file that cannot be decoded is a broken corpus, not a non-rule:
    /// it is an error naming the file, never a silent skip.
    #[test]
    fn an_unreadable_rule_file_is_an_io_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("rules/base")).expect("create category");
        std::fs::write(root.join("rules/base/binary.md"), [0xff, 0xfe, 0x00])
            .expect("write binary");

        let error = build_rules_index(root).expect_err("non-UTF-8 rule must fail");
        match error {
            AppError::Io { message } => {
                assert!(message.contains("binary.md"), "{message}");
                assert!(message.contains("UTF-8"), "{message}");
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }

    /// A README under `rules/` has no frontmatter and no slug, so there is
    /// nothing to key it by — skipped, and it must not abort the catalog.
    #[test]
    fn a_file_that_is_not_a_rule_is_skipped_not_fatal() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("rules/base")).expect("create category");
        std::fs::write(
            root.join("rules/base/README.md"),
            "# Rules\n\nNo frontmatter.\n",
        )
        .expect("write readme");
        write_rule(root, "rules/base/ok.md", "ok");

        let index = build_rules_index(root).expect("a non-rule file must not abort the index");
        assert_eq!(index.keys().collect::<Vec<_>>(), vec!["ok"]);
    }

    /// The vendored seed itself must be a complete, valid `rules/` tree — the
    /// synthetic tests above prove the mechanism, this proves the artifact that
    /// actually ships. A re-vendor should update the count deliberately.
    #[test]
    fn the_vendored_baseline_indexes_as_a_complete_corpus() {
        let baseline = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/rules-baseline");
        let index = build_rules_index(&baseline).expect("the vendored baseline indexes");
        assert_eq!(
            index.len(),
            8,
            "the bundled seed must carry the upstream corpus (8 rules)"
        );
        // Proof that the fingerprint of the real corpus survives the vendor:
        // slug ≠ filename in two places.
        assert!(index.contains_key("commit-conventions"));
        assert!(index.contains_key("python"));
        for (slug, entry) in &index {
            assert!(
                entry.validation.valid,
                "{slug} ships invalid: {:?}",
                entry.validation.errors
            );
        }
        // The category counts the Rules workspace renders must come from the
        // index, not from a restated list (issue #66): the bundled corpus ships
        // exactly `base` = 6 and `stacks` = 2.
        let mut by_category: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for entry in index.values() {
            *by_category.entry(entry.category.as_str()).or_default() += 1;
        }
        assert_eq!(
            by_category,
            std::collections::BTreeMap::from([("base", 6), ("stacks", 2)]),
            "the bundled rule corpus changed shape; update the landing counts"
        );
    }

    // ── roots ───────────────────────────────────────────────────────────────

    #[test]
    fn rules_roots_never_overlap_the_agent_corpus() {
        let app_data = Path::new("/app/data");
        assert_eq!(
            rules_corpus_dir(app_data),
            app_data.join("rules-corpus"),
            "the rules working copy has its own root"
        );
        assert_eq!(
            rules_index_path(app_data),
            app_data.join("state/rules-index.json")
        );
        assert_eq!(
            rules_meta_path(app_data),
            app_data.join("state/rules-meta.json")
        );
        assert_ne!(
            rules_corpus_dir(app_data),
            crate::corpus::corpus_dir(app_data),
            "the two corpora refresh independently and must not share a root"
        );
        assert_ne!(
            rules_index_path(app_data),
            app_data.join("state/corpus-index.json")
        );
        assert_ne!(
            rules_meta_path(app_data),
            app_data.join("state/corpus-meta.json")
        );
    }

    // ── seed and status ─────────────────────────────────────────────────────

    /// No corpus and no usable baseline: "not synced", never an error, never
    /// confused with "the catalog ships no rules".
    #[tokio::test]
    async fn status_reports_not_synced_when_nothing_was_ever_synced() {
        let app_data = tempfile::tempdir().expect("tempdir");
        let baseline = tempfile::tempdir().expect("tempdir");

        let status = status(app_data.path(), baseline.path())
            .await
            .expect("status never fails for a missing corpus");
        assert!(!status.configured);
        assert_eq!(status.count, 0);
        assert_eq!(status.version, "baseline");
        assert_eq!(status.empty_reason, Some(RuleEmptyReason::NotSynced));
    }

    /// A real corpus root that scans to zero rules is the OTHER empty state.
    #[tokio::test]
    async fn status_distinguishes_no_rules_shipped_from_not_synced() {
        let app_data = tempfile::tempdir().expect("tempdir");
        let baseline = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(rules_corpus_dir(app_data.path()).join("rules"))
            .expect("create empty rules dir");

        let status = status(app_data.path(), baseline.path())
            .await
            .expect("status reads an empty rules tree");
        assert!(status.configured, "a rules/ tree exists");
        assert_eq!(status.count, 0);
        assert_eq!(status.empty_reason, Some(RuleEmptyReason::NoRules));
    }

    /// The wire contract the UI branches on: two empty states that cannot be
    /// confused, serialized exactly as `src/lib/types.ts` will model them.
    #[test]
    fn status_serializes_the_two_empty_states_distinctly() {
        let not_synced = RuleStatus {
            configured: false,
            count: 0,
            version: "baseline".into(),
            empty_reason: Some(RuleEmptyReason::NotSynced),
        };
        let no_rules = RuleStatus {
            configured: true,
            count: 0,
            version: "abc1234".into(),
            empty_reason: Some(RuleEmptyReason::NoRules),
        };

        let a = serde_json::to_value(&not_synced).expect("serialize");
        let b = serde_json::to_value(&no_rules).expect("serialize");
        assert_eq!(a["configured"], false);
        assert_eq!(a["emptyReason"], "notSynced");
        assert_eq!(a["version"], "baseline");
        assert_eq!(a["count"], 0);
        assert_eq!(b["configured"], true);
        assert_eq!(b["emptyReason"], "noRules");
        assert_eq!(b["version"], "abc1234");
    }

    /// First run, offline: the bundled baseline is enough for a usable index.
    #[tokio::test]
    async fn the_bundled_baseline_seeds_a_usable_index_offline() {
        let app_data = tempfile::tempdir().expect("tempdir");
        let baseline = tempfile::tempdir().expect("tempdir");
        write_rule(
            baseline.path(),
            "rules/base/coding-standards.md",
            "coding-standards",
        );
        write_rule(baseline.path(), "rules/stacks/typescript.md", "typescript");
        std::fs::write(baseline.path().join("rules/_schema.json"), "{}").expect("write schema");

        let status = status(app_data.path(), baseline.path())
            .await
            .expect("seeded status");
        assert!(status.configured);
        assert_eq!(status.count, 2);
        assert_eq!(status.version, "baseline");
        assert_eq!(status.empty_reason, None);

        // The working copy and the persisted index both exist, offline.
        assert!(rules_corpus_dir(app_data.path())
            .join("rules/base/coding-standards.md")
            .is_file());
        assert!(rules_corpus_dir(app_data.path())
            .join("rules/_schema.json")
            .is_file());
        let raw = std::fs::read(rules_index_path(app_data.path())).expect("index on disk");
        let index: serde_json::Value = serde_json::from_slice(&raw).expect("index parses");
        assert_eq!(index["count"], 2);
        assert!(index["rules"]["typescript"].is_object());
    }

    /// The seed must never stamp the baseline over a refreshed working copy.
    #[tokio::test]
    async fn seeding_is_a_no_op_on_a_populated_root() {
        let app_data = tempfile::tempdir().expect("tempdir");
        let baseline = tempfile::tempdir().expect("tempdir");
        write_rule(
            baseline.path(),
            "rules/base/coding-standards.md",
            "coding-standards",
        );
        write_rule(
            &rules_corpus_dir(app_data.path()),
            "rules/base/local-only.md",
            "local-only",
        );

        let status = status(app_data.path(), baseline.path())
            .await
            .expect("status");
        assert_eq!(status.count, 1);
        let index = build_rules_index(&rules_corpus_dir(app_data.path())).expect("index");
        assert!(index.contains_key("local-only"));
        assert!(
            !index.contains_key("coding-standards"),
            "a populated root must not be re-seeded from the baseline"
        );
    }

    // ── refresh (fixture tarball, no network) ───────────────────────────────

    #[tokio::test]
    async fn install_tarball_swaps_the_tree_and_rebuilds_the_index() {
        let app_data = tempfile::tempdir().expect("tempdir");
        // A pre-existing sync that upstream no longer ships proves the swap
        // replaces the tree rather than merging into it.
        write_rule(
            &rules_corpus_dir(app_data.path()),
            "rules/base/obsolete.md",
            "obsolete",
        );

        let bytes = tarball(&[
            ("rules/base/x.md", RULE_X),
            ("rules/_schema.json", "{}"),
            ("docs/RULE-SPEC.md", "# Rule spec\n"),
            ("tools.json", "{}"),
        ]);
        let written = install_tarball(app_data.path(), &bytes, "deadbeefcafe")
            .await
            .expect("install");
        assert_eq!(written, 1, "only rules/** counts as a rule file");

        let root = rules_corpus_dir(app_data.path());
        assert!(root.join("rules/base/x.md").is_file());
        assert!(root.join("rules/_schema.json").is_file());
        assert!(
            !root.join("rules/base/obsolete.md").exists(),
            "the swap replaces the working copy"
        );
        assert!(!root.join("docs").exists(), "only rules/** is kept");
        assert!(!root.join("tools.json").exists());

        let index: serde_json::Value = serde_json::from_slice(
            &std::fs::read(rules_index_path(app_data.path())).expect("index on disk"),
        )
        .expect("index parses");
        assert_eq!(index["count"], 1);
        assert!(index["rules"]["x"].is_object());

        let meta: StoredRulesMeta = serde_json::from_slice(
            &std::fs::read(rules_meta_path(app_data.path())).expect("meta on disk"),
        )
        .expect("meta parses");
        assert_eq!(meta.version, "deadbeefcafe");
        assert_eq!(meta.count, 1);
        assert!(!meta.fetched_at.is_empty());

        assert!(
            !staging_dir(app_data.path()).exists(),
            "the staging dir must not survive a successful swap"
        );
        assert!(!backup_dir(app_data.path()).exists());
    }

    /// Every file under `root` and its bytes, so "unchanged" is provable rather
    /// than asserted on one path.
    fn tree_contents(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else if let Ok(bytes) = std::fs::read(&path) {
                    let relative = path
                        .strip_prefix(root)
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    out.insert(relative, bytes);
                }
            }
        }
        walk(root, root, &mut out);
        out
    }

    #[tokio::test]
    async fn a_tarball_of_unparsable_rules_never_replaces_a_working_corpus() {
        let app_data = tempfile::tempdir().expect("tempdir");
        write_rule(
            &rules_corpus_dir(app_data.path()),
            "rules/base/good.md",
            RULE_X,
        );
        let before = tree_contents(&rules_corpus_dir(app_data.path()));

        // These files *look* like rules (so `primary` is non-zero) but carry no
        // frontmatter, so the index keeps nothing. Accepting this would swap a
        // healthy corpus for an empty one and report `noRules`.
        let bytes = tarball(&[
            ("rules/base/broken.md", "# no frontmatter here\n"),
            ("rules/_schema.json", "{}"),
        ]);
        let error = install_tarball(app_data.path(), &bytes, "deadbeefcafe")
            .await
            .expect_err("a corpus with zero indexable rules is not an upgrade");
        assert!(
            error.to_string().contains("no indexable rules"),
            "unexpected error: {error}"
        );

        assert_eq!(
            tree_contents(&rules_corpus_dir(app_data.path())),
            before,
            "the working corpus must be untouched"
        );
        assert!(
            !staging_dir(app_data.path()).exists(),
            "staging must be cleaned up"
        );
    }

    #[tokio::test]
    async fn an_unwritable_state_dir_fails_before_the_corpus_is_swapped() {
        let app_data = tempfile::tempdir().expect("tempdir");
        write_rule(
            &rules_corpus_dir(app_data.path()),
            "rules/base/good.md",
            RULE_X,
        );
        let before = tree_contents(&rules_corpus_dir(app_data.path()));

        // Block the state write deterministically, identically under root: the
        // state path is a regular file, so `create_dir_all` cannot succeed.
        std::fs::write(crate::corpus::state_dir(app_data.path()), b"not a dir")
            .expect("occupy the state path");

        let bytes = tarball(&[("rules/base/x.md", RULE_X)]);
        install_tarball(app_data.path(), &bytes, "deadbeefcafe")
            .await
            .expect_err("the state write must fail");

        assert_eq!(
            tree_contents(&rules_corpus_dir(app_data.path())),
            before,
            "a failed state write must not leave the new corpus live"
        );
        assert!(
            !staging_dir(app_data.path()).exists(),
            "staging must be cleaned up"
        );
    }

    #[tokio::test]
    async fn install_tarball_rejects_paths_that_escape_the_root() {
        let parent = tempfile::tempdir().expect("tempdir");
        let app_data = parent.path().join("app-data");
        std::fs::create_dir_all(&app_data).expect("create app data");

        let bytes = raw_named_tarball(&[
            ("agency-rules-main/rules/base/x.md", RULE_X),
            ("agency-rules-main/../../evil.md", "pwned"),
            ("agency-rules-main/rules/../../../etc/evil.md", "pwned"),
            ("/etc/evil.md", "pwned"),
        ]);
        let written = install_tarball(&app_data, &bytes, "abc1234")
            .await
            .expect("unsafe entries are rejected, the rest still installs");
        assert_eq!(written, 1);

        assert!(rules_corpus_dir(&app_data)
            .join("rules/base/x.md")
            .is_file());
        assert!(
            !parent.path().join("evil.md").exists(),
            "a `..` entry must never be written outside the destination"
        );
        assert!(!app_data.join("evil.md").exists());
        assert!(!rules_corpus_dir(&app_data).join("evil.md").exists());
    }

    #[tokio::test]
    async fn a_tarball_with_no_rules_is_an_error_and_leaves_the_corpus_alone() {
        let app_data = tempfile::tempdir().expect("tempdir");
        let bytes = tarball(&[("README.md", "# not a rules repo\n"), ("tools.json", "{}")]);

        let error = install_tarball(app_data.path(), &bytes, "abc1234")
            .await
            .expect_err("a rules tarball with no rules/ must fail");
        match error {
            AppError::Internal { message } => {
                assert!(message.contains("no rule files under rules/"), "{message}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
        assert!(!rules_corpus_dir(app_data.path()).exists());
        assert!(!staging_dir(app_data.path()).exists());
    }

    /// Fail closed: a tarball that cannot become a corpus must never replace a
    /// working one. Duplicate slugs are the realistic case (upstream treats
    /// uniqueness as fatal), and the live tree plus its index must survive it.
    #[tokio::test]
    async fn a_rejected_tarball_never_replaces_a_working_corpus() {
        let app_data = tempfile::tempdir().expect("tempdir");
        write_rule(
            &rules_corpus_dir(app_data.path()),
            "rules/base/keep.md",
            "keep",
        );
        let duplicate = "---\nname: dup\ndescription: A rule.\n---\n\n# Dup\n";
        let bytes = tarball(&[("rules/a/dup.md", duplicate), ("rules/b/dup.md", duplicate)]);

        let error = install_tarball(app_data.path(), &bytes, "deadbeef")
            .await
            .expect_err("duplicate slugs must abort the refresh");
        assert!(
            matches!(error, AppError::InvalidArgument { .. }),
            "{error:?}"
        );

        let root = rules_corpus_dir(app_data.path());
        assert!(
            root.join("rules/base/keep.md").is_file(),
            "the live corpus must be untouched"
        );
        assert!(!root.join("rules/a").exists());
        assert!(!staging_dir(app_data.path()).exists());
        let index = build_rules_index(&root).expect("live corpus still indexes");
        assert_eq!(index.keys().collect::<Vec<_>>(), vec!["keep"]);
    }

    /// Manual gate for TASK 12: the real download → staging → atomic swap →
    /// index rebuild path against the live repository. `#[ignore]`d because CI
    /// must never depend on GitHub; run it with
    /// `cargo test --lib rules_refresh_against_the_live_repo -- --ignored`.
    #[tokio::test]
    #[ignore = "network: hits codeload.github.com and the GitHub API"]
    async fn rules_refresh_against_the_live_repo() {
        let app_data = tempfile::tempdir().expect("tempdir");

        let status = refresh(app_data.path()).await.expect("live refresh");

        assert!(status.configured);
        assert_eq!(status.count, 8, "the upstream corpus ships 8 rules");
        assert!(status.empty_reason.is_none());
        assert!(
            status.version.len() >= 7 && status.version != "baseline",
            "a refresh must record the resolved commit, got {:?}",
            status.version
        );
        let root = rules_corpus_dir(app_data.path());
        assert!(root.join("rules/base/commit-convention.md").is_file());
        assert!(root.join("rules/_schema.json").is_file());
        assert!(!root.join("integrations").exists(), "only rules/** is kept");
    }

    /// After a refresh the status reports the refreshed version and count,
    /// which is what the UI's "synced" state is derived from.
    #[tokio::test]
    async fn status_after_a_refresh_reports_the_new_version() {
        let app_data = tempfile::tempdir().expect("tempdir");
        install_tarball(
            app_data.path(),
            &tarball(&[("rules/base/x.md", RULE_X)]),
            "abc1234",
        )
        .await
        .expect("install");

        let status = read_status(app_data.path()).await.expect("status");
        assert!(status.configured);
        assert_eq!(status.count, 1);
        assert_eq!(status.version, "abc1234");
        assert_eq!(status.empty_reason, None);
    }
}
