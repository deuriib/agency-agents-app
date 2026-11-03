//! `opencode.json` plugin registration — ruling **A6** / TASK 16b.
//!
//! An OpenCode rule written to `.opencode/rules/` is inert unless
//! `opencode.json` registers the `opencode-rules` plugin. Installing a rule for
//! the `opencode` harness therefore has one side effect beyond the rule file:
//! ensure the plugin entry exists.
//!
//! **This module is the only code allowed to touch `opencode.json`.** The
//! renderers stay pure (`rules/render.rs`), and `rules/commands.rs` orchestrates
//! — it never parses or writes this file itself.
//!
//! Two invariants from the rulings shape the whole design:
//!
//! - **C2 — `opencode.json` is never a tracked destination.** It is absent from
//!   a record's `dest`/`dests`, because `rules_uninstall` deletes every path in
//!   `dests` and doing that to a user's config would delete their OpenCode
//!   setup. Registration is reported as a boolean on the install result
//!   ([`PluginRegistration`]) and uninstall never removes the entry.
//! - **A6 — never clobber a user's config.** Unreadable, unparseable, or
//!   wrongly-shaped files abort the install with **zero** writes. Nothing is
//!   coerced, no key is dropped, no key is reordered (`serde_json` is built with
//!   `preserve_order`).
//!
//! The entry string and the directory it lands in both come from the embedded
//! contract (`data/rules-tools.json`), never from a literal in this file.

use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::rules::registry;
use crate::types::RulePlugin;

/// The harness this module serves. Not a destination and not a format — just the
/// contract key, so the id appears once in this module.
pub const HARNESS: &str = "opencode";

/// The OpenCode config file name, at the project root and inside the user
/// config dir.
const CONFIG_FILE: &str = "opencode.json";

/// What the merge decided. Returned by [`plan_merge`] (pure) and consumed by
/// [`apply`] (the only writer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeAction {
    /// The file already registers a matching plugin entry — write nothing, so
    /// the file's bytes and mtime are untouched.
    NoOp { path: PathBuf },
    /// Create or extend the document.
    Write { path: PathBuf, bytes: Vec<u8> },
}

impl MergeAction {
    /// The target path, for both arms.
    pub fn path(&self) -> &Path {
        match self {
            MergeAction::NoOp { path } | MergeAction::Write { path, .. } => path,
        }
    }
}

/// Outcome of a registration attempt, reported separately from the ledger
/// record's destinations (ruling C2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRegistration {
    /// True when `opencode.json` now registers the plugin. Always true on
    /// success — the merge either found the entry or wrote it — but named
    /// explicitly so the install result does not imply it from "no error".
    pub registered: bool,
    /// True when this call created or extended the file; false when the entry
    /// was already present and nothing was touched.
    pub written: bool,
    /// The config file the merge targeted.
    pub path: PathBuf,
}

/// The `plugin.entry` value from the contract.
///
/// `None` in the contract is a build-time authoring error, not a runtime
/// condition, so it maps to [`AppError::Internal`] — the same class as the
/// malformed-contract panic in `rules/registry.rs`.
pub fn contract_entry() -> Result<&'static str, AppError> {
    registry::plugin_entry(HARNESS).ok_or_else(|| AppError::Internal {
        message: format!("rules-tools.json: {HARNESS} has no plugin.entry"),
    })
}

/// The plugin block from the contract, for the pieces [`contract_entry`] does
/// not expose (the package name in the uninstall copy).
fn contract_plugin() -> Result<&'static RulePlugin, AppError> {
    registry::get(HARNESS)
        .and_then(|meta| meta.plugin.as_ref())
        .ok_or_else(|| AppError::Internal {
            message: format!("rules-tools.json: {HARNESS} has no plugin block"),
        })
}

/// The OpenCode config directory for a scope.
///
/// - **project** → `<project_root>/opencode.json` — OpenCode reads the project
///   config from the project root (`docs/rules-integration-architecture.md` §8).
/// - **user** → `<user_base>/<detect dir>/opencode.json`, where the detect dir
///   comes from the contract's `detect.dirs` (`~/.config/opencode` for the
///   shipped contract). `user_base` is the harness base resolved by
///   `rules::render::harness_base`, so a `settings.tool_paths` override for the
///   `opencode` tool moves the plugin config together with the rule files
///   (ruling A7).
///
/// The user config dir is read from the contract instead of being spelled out
/// here: the contract already declares where OpenCode lives, and a second copy
/// of that path is exactly the drift `registry.rs:12-14` forbids.
pub fn config_path(user_base: &Path, project_root: Option<&Path>) -> Result<PathBuf, AppError> {
    match project_root {
        Some(root) => Ok(root.join(CONFIG_FILE)),
        None => Ok(user_base.join(user_config_dir()?).join(CONFIG_FILE)),
    }
}

/// The `~`-relative user config dir the contract declares for [`HARNESS`].
fn user_config_dir() -> Result<PathBuf, AppError> {
    let meta = registry::get(HARNESS).ok_or_else(|| AppError::Internal {
        message: format!("rules-tools.json: {HARNESS} is not declared"),
    })?;
    meta.detect
        .as_ref()
        .and_then(|detect| detect.get("dirs"))
        .and_then(|dirs| dirs.as_array())
        .and_then(|dirs| {
            dirs.iter()
                .filter_map(|dir| dir.as_str())
                .find_map(|dir| dir.strip_prefix("~/"))
        })
        .map(PathBuf::from)
        .ok_or_else(|| AppError::Internal {
            message: format!(
                "rules-tools.json: {HARNESS} declares no `~/` directory in detect.dirs"
            ),
        })
}

/// True when `path` is an OpenCode config file.
///
/// Used as the C2 guard at the ledger write boundary: a rule record must never
/// list this file in `dest`/`dests`, or uninstall would delete a user config.
/// Rule artifacts are `{{slug}}.md`, so this cannot reject a legitimate rule
/// file.
pub fn is_plugin_config(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == std::ffi::OsStr::new(CONFIG_FILE))
}

/// The substring an already-registered entry must contain to count as the same
/// plugin — the contract's `plugin.package` (`opencode-rules`), never a literal.
///
/// Keying on the package rather than on the full entry is what lets a rule
/// install recognise an entry an earlier version wrote: `opencode-rules@0.4.0`
/// already satisfies ruling A6's "any entry whose string contains
/// `opencode-rules`", so nothing is appended next to it.
fn plugin_marker() -> Result<&'static str, AppError> {
    Ok(&contract_plugin()?.package)
}

/// Decide what to write for `path`. **Pure**: reads only `existing` (`None` =
/// the file is absent) and the harness contract already parsed into memory, and
/// performs no I/O — so every failure branch is testable without a filesystem.
///
/// Failure branches, all with zero writes:
/// - bytes that are not valid JSON (including an empty file) →
///   [`AppError::Io`] `parse {path}: {e}`
/// - a document that is not a JSON object → [`AppError::InvalidArgument`]
/// - a `plugin` key that is present but not an array → [`AppError::InvalidArgument`]
///   (never coerced, never overwritten)
///
/// `entry` is the value to append; the caller supplies it from
/// [`contract_entry`], and the match marker is the contract's
/// `plugin.package`, so neither is a literal in the merge logic.
pub fn plan_merge(
    path: PathBuf,
    existing: Option<&[u8]>,
    entry: &str,
) -> Result<MergeAction, AppError> {
    let Some(raw) = existing else {
        let mut document = serde_json::Map::new();
        document.insert(
            "plugin".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String(entry.to_string())]),
        );
        return Ok(MergeAction::Write {
            bytes: document_bytes(&serde_json::Value::Object(document))?,
            path,
        });
    };

    let mut document: serde_json::Value =
        serde_json::from_slice(raw).map_err(|e| AppError::Io {
            message: format!("parse {}: {e}", path.display()),
        })?;

    let object = document
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidArgument {
            message: format!(
                "{} is not a JSON object; refusing to modify it",
                path.display()
            ),
        })?;

    let marker = plugin_marker()?;
    match object.get_mut("plugin") {
        None => {
            object.insert(
                "plugin".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::String(entry.to_string())]),
            );
        }
        Some(serde_json::Value::Array(plugins)) => {
            // Matching ignores non-string elements (an object or number in the
            // array is preserved untouched, never inspected) and a substring
            // match is enough: `opencode-rules@0.4.0` from an earlier install
            // must not get a second, pinned entry appended next to it.
            let already_present = plugins.iter().any(|plugin| match plugin {
                serde_json::Value::String(value) => value.contains(marker),
                _ => false,
            });
            if already_present {
                return Ok(MergeAction::NoOp { path });
            }
            plugins.push(serde_json::Value::String(entry.to_string()));
        }
        Some(_) => {
            return Err(AppError::InvalidArgument {
                message: format!(
                    "\"plugin\" in {} is not an array; refusing to modify it",
                    path.display()
                ),
            });
        }
    }

    Ok(MergeAction::Write {
        bytes: document_bytes(&document)?,
        path,
    })
}

/// Serialize a document the way this app writes JSON everywhere else: 2-space
/// indent (`serde_json::to_string_pretty`) plus one trailing newline.
///
/// The trailing newline is **normalized**: a file that ended without one gains
/// it, and a file with several trailing newlines ends with exactly one. That is
/// whitespace, not content, and it is recorded as accepted in
/// `docs/rules-integration-architecture.md` §8 alongside the indent
/// normalization.
fn document_bytes(document: &serde_json::Value) -> Result<Vec<u8>, AppError> {
    let mut text = serde_json::to_string_pretty(document).map_err(|e| AppError::Internal {
        message: format!("serialize {CONFIG_FILE}: {e}"),
    })?;
    text.push('\n');
    Ok(text.into_bytes())
}

/// Perform the write a [`MergeAction`] decided on. The only I/O in this module.
///
/// `NoOp` does nothing at all — no empty rewrite, so the file's bytes and mtime
/// are untouched. `Write` creates the parent directory when it is missing (a
/// first-time user scope install) and then writes through
/// [`crate::util::fs::atomic_write`] (temp file in the same directory, then
/// rename), so a crash never leaves a torn config.
pub async fn apply(action: &MergeAction) -> Result<(), AppError> {
    let MergeAction::Write { path, bytes } = action else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Io {
                message: format!("create {}: {e}", parent.display()),
            })?;
    }
    crate::util::fs::atomic_write(path, bytes).await
}

/// Read → plan → apply. Returns whether the entry had to be written; a
/// `NoOp` still reports `registered: true`.
pub async fn ensure_registered(path: &Path, entry: &str) -> Result<PluginRegistration, AppError> {
    let existing = match tokio::fs::read(path).await {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(AppError::Io {
                message: format!("read {}: {e}", path.display()),
            })
        }
    };
    let action = plan_merge(path.to_path_buf(), existing.as_deref(), entry)?;
    let written = matches!(action, MergeAction::Write { .. });
    apply(&action).await?;
    Ok(PluginRegistration {
        registered: true,
        written,
        path: path.to_path_buf(),
    })
}

/// The whole OpenCode step of an install: resolve the path for the scope, read
/// the entry from the contract, and register it.
///
/// This is the one call `rules::commands::rules_install` makes. It must run
/// **before** the rule file is written (architecture §4, steps 9 → 10): a
/// failure here leaves no rule behind, and a retry is safe because the merge is
/// idempotent.
pub async fn register_for_install(
    user_base: &Path,
    project_root: Option<&Path>,
) -> Result<PluginRegistration, AppError> {
    let entry = contract_entry()?;
    let path = config_path(user_base, project_root)?;
    ensure_registered(&path, entry).await
}

/// The copy the UI must show after uninstalling an OpenCode rule: the rule file
/// and the ledger row go, the plugin entry stays (other rules may depend on it).
///
/// The package name comes from the contract so the sentence follows a rename.
/// W8 owns the translations; this is the fact they must carry.
pub fn uninstall_note() -> Result<String, AppError> {
    let package = &contract_plugin()?.package;
    Ok(format!(
        "the {package} plugin entry stays in {CONFIG_FILE}; other rules may depend on it"
    ))
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "opencode-rules@latest";

    fn plan(path: &Path, existing: Option<&str>) -> Result<MergeAction, AppError> {
        plan_merge(path.to_path_buf(), existing.map(str::as_bytes), ENTRY)
    }

    fn written_bytes(action: &MergeAction) -> &[u8] {
        match action {
            MergeAction::Write { bytes, .. } => bytes,
            MergeAction::NoOp { .. } => panic!("expected a write, got NoOp"),
        }
    }

    #[test]
    fn missing_file_is_created_with_only_the_plugin_key() {
        let path = PathBuf::from("/tmp/opencode.json");
        let action = plan(&path, None).expect("a missing file is a write, not an error");
        assert_eq!(
            String::from_utf8_lossy(written_bytes(&action)),
            "{\n  \"plugin\": [\n    \"opencode-rules@latest\"\n  ]\n}\n"
        );
    }

    #[test]
    fn existing_file_without_plugin_gains_the_key_without_reordering() {
        let path = PathBuf::from("/tmp/opencode.json");
        let existing = "{\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \"model\": \"a/b\",\n  \"theme\": \"tokyonight\"\n}\n";
        let action = plan(&path, Some(existing)).expect("a missing plugin key is a write");
        let out = String::from_utf8_lossy(written_bytes(&action)).into_owned();

        // Every pre-existing key survives, in its original order, and `plugin`
        // is appended after them (preserve_order — a BTreeMap would sort these).
        let schema = out.find("$schema").expect("$schema kept");
        let model = out.find("\"model\"").expect("model kept");
        let theme = out.find("\"theme\"").expect("theme kept");
        let plugin = out.find("\"plugin\"").expect("plugin added");
        assert!(schema < model && model < theme && theme < plugin, "{out}");
        assert!(out.contains("\"model\": \"a/b\""), "{out}");
        assert!(out.contains("\"opencode-rules@latest\""), "{out}");
    }

    #[test]
    fn plugin_already_present_is_a_noop_without_touching_anything() {
        let path = PathBuf::from("/tmp/opencode.json");
        // Exact entry.
        let exact = "{\n  \"plugin\": [\n    \"opencode-rules@latest\"\n  ]\n}\n";
        assert_eq!(
            plan(&path, Some(exact)).expect("valid json"),
            MergeAction::NoOp { path: path.clone() }
        );
        // Substring: an entry written by an earlier install version.
        let older = "{\n  \"plugin\": [\n    \"opencode-rules@0.4.0\"\n  ]\n}\n";
        assert_eq!(
            plan(&path, Some(older)).expect("valid json"),
            MergeAction::NoOp { path: path.clone() }
        );
        // Buried among other plugins and non-string entries: still recognised,
        // and the array is not rewritten.
        let mixed = "{\n  \"plugin\": [\n    42,\n    \"other-plugin@1\",\n    \"opencode-rules@1.2.3\"\n  ]\n}\n";
        assert!(matches!(
            plan(&path, Some(mixed)).expect("valid json"),
            MergeAction::NoOp { .. }
        ));
    }

    #[test]
    fn plugin_absent_from_a_mixed_array_is_appended_and_preserved() {
        let path = PathBuf::from("/tmp/opencode.json");
        let existing = "{\n  \"plugin\": [\n    42,\n    \"other-plugin@1\"\n  ]\n}\n";
        let out =
            String::from_utf8_lossy(written_bytes(&plan(&path, Some(existing)).expect("write")))
                .into_owned();
        assert!(out.contains("42"), "non-string element preserved: {out}");
        assert!(out.contains("\"other-plugin@1\""), "{out}");
        assert!(out.contains("\"opencode-rules@latest\""), "{out}");
    }

    #[test]
    fn invalid_json_aborts_with_a_parse_error() {
        let path = PathBuf::from("/tmp/opencode.json");
        match plan(&path, Some("{not json")) {
            // The contract is the `parse {path}: ` prefix (architecture §10);
            // the validator's own wording after it is not ours to pin.
            Err(AppError::Io { message }) => {
                assert!(
                    message.starts_with("parse /tmp/opencode.json: "),
                    "{message}"
                )
            }
            other => panic!("expected an Io parse error, got {other:?}"),
        }
    }

    /// A zero-byte file is not an absent file: it is unparseable, so the merge
    /// fails closed instead of overwriting something the user may be mid-edit
    /// on. Only a genuinely missing file is created (ruling A6).
    #[test]
    fn an_empty_file_aborts_instead_of_being_treated_as_absent() {
        let path = PathBuf::from("/tmp/opencode.json");
        assert!(matches!(plan(&path, Some("")), Err(AppError::Io { .. })));
    }

    #[test]
    fn a_non_object_document_aborts() {
        let path = PathBuf::from("/tmp/opencode.json");
        for raw in ["[1, 2]", "\"a string\"", "null"] {
            match plan(&path, Some(raw)) {
                Err(AppError::InvalidArgument { message }) => assert_eq!(
                    message,
                    "/tmp/opencode.json is not a JSON object; refusing to modify it"
                ),
                other => panic!("expected InvalidArgument for {raw}, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_non_array_plugin_key_aborts_instead_of_being_coerced() {
        let path = PathBuf::from("/tmp/opencode.json");
        let existing = "{\n  \"plugin\": \"opencode-rules@latest\"\n}\n";
        match plan(&path, Some(existing)) {
            Err(AppError::InvalidArgument { message }) => assert_eq!(
                message,
                "\"plugin\" in /tmp/opencode.json is not an array; refusing to modify it"
            ),
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    #[test]
    fn output_uses_two_space_indent_and_one_trailing_newline() {
        let path = PathBuf::from("/tmp/opencode.json");
        let out = written_bytes(&plan(&path, None).expect("write")).to_vec();
        let text = String::from_utf8(out.clone()).expect("utf-8");
        assert!(text.starts_with("{\n  \"plugin\": [\n    \""), "{text:?}");
        assert!(text.ends_with("]\n}\n"), "{text:?}");
        assert!(!text.ends_with("\n\n"), "exactly one trailing newline");
        assert!(!text.contains('\t'), "no tabs in a 2-space document");
    }

    #[test]
    fn a_file_without_a_trailing_newline_is_normalized_to_one() {
        let path = PathBuf::from("/tmp/opencode.json");
        let existing = "{\"model\":\"a/b\"}";
        let text =
            String::from_utf8_lossy(written_bytes(&plan(&path, Some(existing)).expect("write")))
                .into_owned();
        assert!(text.ends_with("}\n"), "{text:?}");
        assert!(!text.ends_with("}\n\n"), "{text:?}");
    }

    /// The entry value is data, not a literal: the contract's own
    /// `plugin.entry` is what gets written, and a different entry produces
    /// different bytes (which is what a contract edit would do).
    #[test]
    fn entry_value_comes_from_the_contract() {
        const CONTRACT: &str = include_str!("../../data/rules-tools.json");
        let contract: serde_json::Value =
            serde_json::from_str(CONTRACT).expect("data/rules-tools.json parses");
        let declared = contract["harnesses"][HARNESS]["plugin"]["entry"]
            .as_str()
            .expect("the opencode harness declares plugin.entry in the contract");

        assert_eq!(contract_entry().expect("contract entry"), declared);

        let path = PathBuf::from("/tmp/opencode.json");
        let written = String::from_utf8_lossy(written_bytes(
            &plan_merge(path.clone(), None, declared).expect("write"),
        ))
        .into_owned();
        assert!(written.contains(declared), "{written}");

        // Prove there is no hardcoded value deciding this: any entry the caller
        // passes is the one written.
        let replaced = String::from_utf8_lossy(written_bytes(
            &plan_merge(path, None, "opencode-rules@9.9.9").expect("write"),
        ))
        .into_owned();
        assert!(replaced.contains("opencode-rules@9.9.9"), "{replaced}");
        assert!(!replaced.contains("latest"), "{replaced}");
    }

    /// Ruling A6's match substring is the contract's `package`, not the entry
    /// and not a literal, so a version bump in the contract still recognises an
    /// entry written by an earlier install.
    #[test]
    fn marker_is_the_contract_package() {
        const CONTRACT: &str = include_str!("../../data/rules-tools.json");
        let contract: serde_json::Value =
            serde_json::from_str(CONTRACT).expect("data/rules-tools.json parses");
        let declared = contract["harnesses"][HARNESS]["plugin"]["package"]
            .as_str()
            .expect("the opencode harness declares plugin.package");
        assert_eq!(plugin_marker().expect("marker"), declared);
        assert_eq!(declared, "opencode-rules");
    }

    #[test]
    fn project_scope_targets_the_project_root_and_user_scope_the_config_dir() {
        let base = Path::new("/Users/example");
        let project = config_path(base, Some(Path::new("/Users/example/projects/api")))
            .expect("project path");
        assert_eq!(
            project,
            PathBuf::from("/Users/example/projects/api/opencode.json")
        );

        let user = config_path(base, None).expect("user path");
        assert_eq!(
            user,
            PathBuf::from("/Users/example/.config/opencode/opencode.json")
        );

        // The user config dir is read from the contract, not spelled out here.
        const CONTRACT: &str = include_str!("../../data/rules-tools.json");
        let contract: serde_json::Value =
            serde_json::from_str(CONTRACT).expect("data/rules-tools.json parses");
        let declared = contract["harnesses"][HARNESS]["detect"]["dirs"]
            .as_array()
            .expect("detect.dirs is an array")
            .iter()
            .filter_map(|dir| dir.as_str())
            .find_map(|dir| dir.strip_prefix("~/"))
            .expect("the contract declares a ~/ config dir");
        let user_dir = user.parent().expect("config file has a parent");
        assert_eq!(user_dir, base.join(declared));
    }

    #[test]
    fn a_tool_paths_override_moves_the_plugin_config_with_the_tool() {
        // Ruling A7: `harness_base` resolves the override, and the config dir is
        // relative to that base.
        let overridden = Path::new("/wsl/home/me");
        assert_eq!(
            config_path(overridden, None).expect("user path"),
            PathBuf::from("/wsl/home/me/.config/opencode/opencode.json")
        );
    }

    #[test]
    fn plugin_config_paths_are_recognised_as_untrackable() {
        assert!(is_plugin_config(Path::new(
            "/Users/example/.config/opencode/opencode.json"
        )));
        assert!(is_plugin_config(Path::new("/p/opencode.json")));
        assert!(!is_plugin_config(Path::new(
            "/Users/example/.config/opencode/rules/typescript.md"
        )));
        assert!(!is_plugin_config(Path::new("/p/opencode.jsonc")));
    }

    #[test]
    fn uninstall_note_names_the_contract_package() {
        let package = &contract_plugin().expect("plugin block").package;
        let note = uninstall_note().expect("note");
        assert!(note.contains(package), "{note}");
        assert!(note.contains("stays in opencode.json"), "{note}");
    }

    // ---------- Filesystem behaviour ----------

    #[tokio::test]
    async fn ensure_registered_creates_missing_directories_and_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested/config/opencode.json");
        let outcome = ensure_registered(&path, ENTRY).await.expect("register");
        assert!(outcome.registered);
        assert!(outcome.written);
        assert_eq!(outcome.path, path);
        let bytes = tokio::fs::read(&path).await.expect("read");
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "{\n  \"plugin\": [\n    \"opencode-rules@latest\"\n  ]\n}\n"
        );
        // Atomic write leaves no temp sibling behind.
        assert!(!dir.path().join("nested/config/opencode.json.tmp").exists());
    }

    #[tokio::test]
    async fn ensure_registered_is_a_noop_when_the_entry_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("opencode.json");
        let original =
            "{\n  \"plugin\": [\n    \"opencode-rules@0.1.0\"\n  ],\n  \"model\": \"a/b\"\n}\n";
        tokio::fs::write(&path, original).await.expect("seed");
        let before = tokio::fs::metadata(&path)
            .await
            .expect("stat")
            .modified()
            .expect("mtime");

        let outcome = ensure_registered(&path, ENTRY).await.expect("register");
        assert!(outcome.registered);
        assert!(!outcome.written, "an existing entry must not be rewritten");
        assert_eq!(
            tokio::fs::read_to_string(&path).await.expect("read"),
            original
        );

        let after = tokio::fs::metadata(&path)
            .await
            .expect("stat")
            .modified()
            .expect("mtime");
        assert_eq!(before, after, "mtime must not change on a NoOp");
    }

    #[tokio::test]
    async fn ensure_registered_leaves_invalid_files_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("opencode.json");
        let original = "{ this is not json";
        tokio::fs::write(&path, original).await.expect("seed");

        let err = ensure_registered(&path, ENTRY)
            .await
            .expect_err("invalid JSON aborts the install");
        assert!(matches!(err, AppError::Io { .. }), "{err:?}");
        assert_eq!(
            tokio::fs::read_to_string(&path).await.expect("read"),
            original,
            "a user's config is never clobbered"
        );
    }

    #[tokio::test]
    async fn ensure_registered_leaves_a_non_array_plugin_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("opencode.json");
        let original = "{\n  \"plugin\": {\n    \"name\": \"opencode-rules\"\n  }\n}\n";
        tokio::fs::write(&path, original).await.expect("seed");

        let err = ensure_registered(&path, ENTRY)
            .await
            .expect_err("a non-array plugin aborts");
        assert!(matches!(err, AppError::InvalidArgument { .. }), "{err:?}");
        assert_eq!(
            tokio::fs::read_to_string(&path).await.expect("read"),
            original
        );
    }

    #[tokio::test]
    async fn register_for_install_reads_the_entry_from_the_contract() {
        let dir = tempfile::tempdir().expect("tempdir");
        let base = dir.path().join("home");
        let outcome = register_for_install(&base, None).await.expect("register");
        assert!(outcome.written);
        let expected = base.join(".config/opencode/opencode.json");
        assert_eq!(outcome.path, expected);
        let text = tokio::fs::read_to_string(&expected).await.expect("read");
        assert!(
            text.contains(contract_entry().expect("contract entry")),
            "{text}"
        );

        // A second install for the same scope writes nothing.
        let again = register_for_install(&base, None)
            .await
            .expect("re-register");
        assert!(!again.written);
    }

    #[tokio::test]
    async fn project_scope_writes_the_config_at_the_project_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let project = dir.path().join("project");
        let outcome = register_for_install(Path::new("/nonexistent-home"), Some(&project))
            .await
            .expect("register");
        assert_eq!(outcome.path, project.join("opencode.json"));
        assert!(outcome.path.exists());
    }
}
