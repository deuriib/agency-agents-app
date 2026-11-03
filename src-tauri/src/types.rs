//! Shared DTOs serialized across the Tauri IPC boundary.
//!
//! Every struct uses `#[serde(rename_all = "camelCase")]` so the
//! TypeScript side matches `src/lib/types.ts` exactly.

use serde::{Deserialize, Serialize};

// =========================================================
// Agency Agents — corpus subsystem (contracts.md §A)
// =========================================================
//
// Wire format mirrors `src/lib/types.ts`.

// ---------- Tools & scope ----------

/// An AI coding tool we can deploy an agent into, identified by its camelCase
/// string id (e.g. `"claudeCode"`, `"geminiCli"`). The id IS the wire value the
/// TS `Tool` union depends on; the authoritative tool set lives in the embedded
/// JSON registry (`crate::registry`) — adding a tool is adding a JSON file, not
/// a Rust variant. Kept as a type alias so every struct field carrying a tool
/// (`InstalledAgent`, `ToolInfo`, `LoadoutEntry`, …) stays wire-compatible.
pub type Tool = String;

/// Deployment scope. User-global tools write to fixed `~/…` dests;
/// project-scoped tools install into a tracked `project_path`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    User,
    Project,
}

// ---------- Catalog source (where the corpus lives) ----------

/// Where the active agent catalog lives on disk. The whole app reads/writes the
/// resolved root, so this is the one knob that says "be a respectful frontend
/// over the user's clone" vs "manage our own copy." Persisted to
/// `state/catalog.json`. Serialized tagged on `kind` so the TS side is a clean
/// discriminated union.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CatalogSource {
    /// App-managed copy seeded from the bundled baseline (`<app_data>/corpus`).
    /// The always-works default; never touches anything outside app data.
    #[default]
    Bundled,
    /// A clone the app provisioned and owns (default `~/.agency-agents`). The
    /// app may pull/refresh it; it's shared with the CLI.
    Managed { path: String },
    /// The user's own pre-existing clone. `manage` records whether the user
    /// granted permission to pull it (manage-with-permission); when false we
    /// only ever read from it.
    UserClone { path: String, manage: bool },
}

/// A catalog directory discovered on disk (for the first-run / Settings picker).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCandidate {
    /// Absolute path to the candidate catalog root.
    pub path: String,
    /// `"managed"` for `~/.agency-agents`, else `"userClone"`.
    pub kind: String,
    /// Whether it's a git checkout (has `.git`) — drives pull strategy.
    pub has_git: bool,
    /// Quick agent count (top-level `.md` across discovered categories).
    pub agent_count: u32,
}

/// Result of `catalog_detect` — what the app found, plus whether `git` is on
/// PATH (so the UI can explain clone vs snapshot provisioning).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDetection {
    pub git_available: bool,
    /// True when a filesystem scan of common dev roots was performed (the
    /// "Find Agency Agents" button), vs the cheap `~/.agency-agents`-only check.
    pub scanned: bool,
    pub candidates: Vec<CatalogCandidate>,
}

/// Live status of the active catalog — source, git provenance, and freshness.
/// Powers the Settings → Catalog panel ("manage the repo": which commit, how
/// far behind, what GitHub repo). All git fields are `None`/0 for a non-git
/// (bundled snapshot) source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatus {
    pub source: CatalogSource,
    /// Catalog root path (None for the bundled, app-data-internal source).
    pub root: Option<String>,
    pub is_git: bool,
    pub branch: Option<String>,
    /// Short commit SHA of HEAD.
    pub commit: Option<String>,
    pub last_commit_subject: Option<String>,
    pub last_commit_date: Option<String>,
    /// Count of uncommitted working-tree changes.
    pub dirty_count: u32,
    /// `origin` remote URL, if a git checkout.
    pub remote_url: Option<String>,
    /// `owner/repo` parsed from the remote (for GitHub repo stats), if it's a
    /// github.com remote.
    pub repo_slug: Option<String>,
    pub version: String,
    pub fetched_at: String,
    pub agent_count: u32,
}

/// Result of checking the active catalog for upstream updates — the "stats on
/// diffs" view. Git sources fetch + compare against the upstream branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogUpdateCheck {
    pub is_git: bool,
    /// Commits the upstream branch has that we don't (how far behind).
    pub behind: u32,
    /// Commits we have that upstream doesn't (local work).
    pub ahead: u32,
    /// Files that would change on pull.
    pub changed_files: u32,
    /// Human-readable `git diff --stat` of HEAD..upstream.
    pub diffstat: String,
    /// True when already at the upstream tip (git) — no-op pull.
    pub up_to_date: bool,
}

// ---------- Agent (parsed from the corpus) ----------

/// An agent as parsed from a single corpus `.md` file. `body` is the
/// markdown persona and is omitted/empty in list views (`corpus_list`)
/// to keep payloads small; `corpus_get` returns it populated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// Filename without `.md`, e.g. `"frontend-developer"`.
    pub slug: String,
    /// Frontmatter `name`.
    pub name: String,
    /// Frontmatter `description`.
    pub description: String,
    /// Parent directory, e.g. `"engineering"`.
    pub category: String,
    /// Frontmatter `emoji`.
    pub emoji: Option<String>,
    /// Frontmatter `color` (named or hex).
    pub color: Option<String>,
    /// Frontmatter `vibe`.
    pub vibe: Option<String>,
    /// Markdown body (persona) — lazy/optional in list views.
    pub body: String,
}

// ---------- Corpus index ----------

/// One row of `corpus-index.json`. The three split hashes let update
/// classification distinguish cosmetic (frontmatter-only) from
/// substantive (body) changes. Hash = SHA-256 lowercase hex of UTF-8
/// bytes (contracts.md §E).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusEntry {
    pub slug: String,
    pub name: String,
    pub category: String,
    pub emoji: Option<String>,
    pub color: Option<String>,
    pub vibe: Option<String>,
    pub description: String,
    /// SHA-256 of the full canonical `.md`.
    pub source_hash: String,
    /// SHA-256 of the frontmatter block.
    pub frontmatter_hash: String,
    /// SHA-256 of the body.
    pub body_hash: String,
}

/// Top-level metadata for the maintained corpus copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusMeta {
    pub version: String,
    pub commit: Option<String>,
    pub fetched_at: String,
    pub count: u32,
}

// ---------- Install ledger ----------

/// One row of `installs.json` — the ledger of local install actions.
/// `source_hash` records the corpus version installed from;
/// `rendered_hash` is the SHA-256 of the exact bytes written after
/// per-tool conversion, used by reconciliation to classify state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub slug: String,
    pub tool: Tool,
    pub scope: Scope,
    pub project_path: Option<String>,
    /// Absolute path written (primary; kept for backward compat with older ledgers).
    pub dest: String,
    /// All absolute paths written for this install (multi-file tools). Empty on
    /// older ledgers; backfilled to `[dest]` at load time.
    #[serde(default)]
    pub dests: Vec<String>,
    /// Per-file SHA-256 for multi-file installs (relative path -> hash). Empty
    /// for single-file tools. `#[serde(default)]` for backward compat.
    #[serde(default)]
    pub file_hashes: std::collections::BTreeMap<String, String>,
    /// For roster/aggregate tools: slugs included in the combined file.
    /// `#[serde(default)]` for backward compat.
    #[serde(default)]
    pub aggregate_slugs: Vec<String>,
    pub source_hash: String,
    /// SHA-256 of the agent body at install time. Lets reconciliation label an
    /// available update cosmetic (body unchanged) vs substantive. `#[serde(default)]`
    /// so ledgers written before this field still parse (older rows get "").
    #[serde(default)]
    pub body_hash: String,
    pub rendered_hash: String,
    pub installed_at: String,
    pub corpus_version: String,
}

// ---------- Reconciliation ----------

/// The five reconciliation states (like a package manager's installed /
/// outdated states). See systemPatterns.md §4 for the disk ↔ ledger ↔ corpus
/// test that classifies each on-disk agent file.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum InstallState {
    Current,
    Outdated,
    Modified,
    Removed,
    Foreign,
}

/// Whether an available update is cosmetic (frontmatter/metadata only,
/// `body_hash` unchanged) or substantive (prompt body changed).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum UpdateKind {
    Cosmetic,
    Substantive,
}

/// Reconciled view-model for the Library — one on-disk agent file
/// resolved against the ledger and corpus-index. `update_kind` is
/// `Some(..)` only when `state == Outdated`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAgent {
    pub slug: String,
    pub name: String,
    pub tool: Tool,
    pub scope: Scope,
    pub project_path: Option<String>,
    pub dest: String,
    pub state: InstallState,
    pub update_kind: Option<UpdateKind>,
    /// True when THIS app installed it (it's in the ledger); false when the
    /// Foreign sweep found it on disk (e.g. a prior `install.sh` run). Lets the
    /// UI distinguish "tracked by the app" from "present from other tools"
    /// instead of claiming every recognized file as "installed by you".
    pub tracked: bool,
}

/// Result of `agent_diff` — what's on disk now vs the canonical render the app
/// would write. Powers "review before Update": the UI can show the user exactly
/// what an Update/Restore would change before any file is touched.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDiff {
    pub slug: String,
    pub tool: Tool,
    pub project_path: Option<String>,
    pub dest: String,
    /// Current on-disk contents (None if the file is missing).
    pub on_disk: Option<String>,
    /// The canonical render the app would write.
    pub proposed: String,
    /// Whether the two differ (false ⇒ Update is a no-op).
    pub differs: bool,
}

// ---------- Tools / categories / projects ----------

/// View-model for the Tools section — a detected AI tool plus its
/// deployment surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub tool: Tool,
    pub label: String,
    pub detected: bool,
    pub scope: Scope,
    pub user_dest: Option<String>,
    pub installed_count: u32,
    /// Per-tool custom install base path the user configured (else `None` =
    /// OS home). Detection + `user_dest` already reflect this base.
    pub custom_path: Option<String>,
}

/// Best-effort detected version string for a tool, from probing `<bin>
/// --version`. `version` is `None` when the binary isn't on PATH, the probe
/// timed out, or the tool has no known version command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersion {
    pub tool: Tool,
    pub version: Option<String>,
}

/// One category for the Discover grid. `slug` is the corpus parent dir
/// (e.g. `"engineering"`); `icon` is a PascalCase Lucide icon name the
/// frontend resolves via its static icon map.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub slug: String,
    pub label: String,
    pub icon: String,
    /// Brand color (hex) for the division, from the catalog metadata.
    pub color: String,
    pub count: u32,
}

/// A registered project directory for project-scoped installs. The app
/// keeps a Projects list so Library/Tools can show per-project
/// deployment; one agent in five projects = five tracked rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    /// Absolute project root path.
    pub path: String,
    /// Display label (defaults to the directory name).
    pub label: String,
    /// Count of agents installed into this project across all
    /// project-scoped tools.
    pub installed_count: u32,
}

// ---------- Rules — the agency-rules corpus (issue #29) ----------
//
// The neutral rule format lives once under `rules/<category>/<name>.md`; the
// four harness variants are compiled, never authored. These DTOs mirror
// `src/lib/types.ts`. Shape contract: `docs/rules-integration-architecture.md`
// §3.
//
// W1 (tasks 1-4) lands these ahead of the renderers, corpus and commands that
// consume them (W2-W6 in the same milestone), so production builds cannot reach
// them yet. `#[allow(dead_code)]` is scoped to these W1 items and goes away as
// each consumer lands — same arrangement as `registry::ToolMeta`.

/// A rule harness id, e.g. `"oh-my-pi"`. A plain string (not an enum) for the
/// same reason [`Tool`] is a string: the set lives in embedded JSON
/// (`data/rules-tools.json`), so adding a harness is a JSON edit — and, per
/// spec §5.3, a harness is only *installable* when a renderer exists for its
/// format.
#[allow(dead_code)]
pub type RuleHarness = String;

/// Authored `globs` in the neutral format accepts BOTH shapes (`rules/_schema.json`
/// uses `oneOf`). Untagged preserves which form was authored: oh-my-pi and
/// OpenCode re-emit the authored shape, while Antigravity always joins with `,`.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleGlobs {
    One(String),
    Many(Vec<String>),
}

/// A fully parsed neutral rule (`rules/<category>/<file>.md`).
///
/// `slug` is the frontmatter `name`, never the filename — the corpus proves it
/// (`rules/base/commit-convention.md` declares `name: commit-conventions`).
/// `relative_path` is required for the same reason: `rules_get` cannot locate a
/// slug's file without it.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// Frontmatter `name` — the identity, and the `{{slug}}` substituted into
    /// every destination template.
    pub slug: String,
    /// Frontmatter `name` again; equal to `slug` by construction. Kept separate
    /// because the renderers emit a display name while the index is keyed by
    /// identity.
    pub name: String,
    pub description: String,
    /// Parent directory name, e.g. `"base"` / `"stacks"`.
    pub category: String,
    /// Corpus-relative source path, e.g. `"rules/base/commit-convention.md"`.
    pub relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub globs: Option<RuleGlobs>,
    /// `Option<bool>`, never `bool`: `alwaysApply: false` must stay
    /// distinguishable from absent, because the oh-my-pi renderer keys on
    /// presence, not truthiness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_apply: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// `match` is a Rust keyword; the serde rename is mandatory — forgetting it
    /// silently drops the field and the OpenCode renderer loses output.
    #[serde(default, rename = "match", skip_serializing_if = "Option::is_none")]
    pub match_: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    /// Markdown body, already `trim()`-ed exactly as the renderers consume it.
    pub body: String,
    /// SHA-256 of the whole file bytes (`render::sha256_hex`).
    pub source_hash: String,
    /// SHA-256 of the normalized body — the bytes that get written.
    pub body_hash: String,
    pub validation: RuleValidation,
}

/// One row of `<app_data>/state/rules-index.json`. Carries everything the list
/// view and every renderer need, so a list render is zero filesystem reads.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEntry {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    /// See [`Rule::relative_path`] — dropping it makes `rules_get` impossible.
    pub relative_path: String,
    pub globs: Option<RuleGlobs>,
    pub always_apply: Option<bool>,
    pub trigger: Option<String>,
    #[serde(rename = "match")]
    pub match_: Option<String>,
    pub tags: Vec<String>,
    pub tools: Vec<String>,
    pub source_hash: String,
    pub body_hash: String,
    /// Invalid rules are RETAINED with their errors so the UI can list them and
    /// explain why install is refused — filtered out of install, not of list.
    pub validation: RuleValidation,
}

/// Pure result of `rules/validate.rs`. `errors` are the schema validator's own
/// messages, unprefixed and untruncated ("surfaced verbatim", spec §8).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl RuleValidation {
    /// A rule that passed the schema gate. Used by callers that build a `Rule`
    /// without the schema (tests/fixtures) so they cannot accidentally claim
    /// `valid` with a non-empty error list.
    #[allow(dead_code)]
    pub fn passed() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
        }
    }
}

/// `rules_status` — the only source of the "not synced" vs "no rules shipped"
/// distinction (spec §8, ruling A5/C8).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleStatus {
    /// A rules corpus root exists and carries a `rules/` tree.
    pub configured: bool,
    /// Rules in the index (invalid ones included — they are listed, not
    /// installed).
    pub count: u32,
    /// The refreshed commit SHA, or `"baseline"` before the first refresh.
    pub version: String,
    /// `null` when there are rules to show.
    pub empty_reason: Option<RuleEmptyReason>,
}

/// Why the rules list is empty. The two arms have different copy and different
/// CTAs in the UI (spec §8): "not synced" offers a sync, "no rules" is reported
/// honestly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuleEmptyReason {
    /// No corpus root at all — first launch before seeding finished, or a
    /// failed seed. CTA: refresh.
    NotSynced,
    /// A corpus root is present, `rules/` was scanned, and it holds zero
    /// parsable rules. CTA: refresh, and say so honestly.
    NoRules,
}

// ---------- Rule harness contract (data/rules-tools.json) ----------

/// One harness, as authored in `data/rules-tools.json` — a verbatim copy of
/// `agency-rules/tools.json`. Mirrors the `registry::ToolMeta` + derived
/// capabilities pattern: the JSON stores what upstream declares; this app
/// derives what it can install.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleHarnessMeta {
    pub id: String,
    pub label: String,
    pub format: String,
    pub install_kind: String,
    pub integration_path: String,
    /// Parsed so the file stays verbatim; nothing consumes it in M1.
    #[serde(default)]
    pub detect: Option<serde_json::Value>,
    /// Parsed so the file stays verbatim; the renderers hard-code their field
    /// sets because byte parity, not this map, is the contract.
    #[serde(default)]
    pub frontmatter: Option<serde_json::Value>,
    /// Parsed so the file stays verbatim (oh-my-pi accepts `.md`/`.mdc`; this
    /// app emits `.md`, which the destination template already carries).
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Consumed by the OpenCode plugin merge (ruling A6). Never hardcode either
    /// value in code.
    #[serde(default)]
    pub plugin: Option<RulePlugin>,
    pub dest: RuleHarnessDest,
}

/// NOTE the shape difference from `registry::Dest`: rule destinations are a
/// SINGLE optional template per scope, not a `Vec`.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuleHarnessDest {
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}

/// The `opencode.plugin` block. `entry` is the string written into
/// `opencode.json`; `package` is the npm package it refers to.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RulePlugin {
    pub package: String,
    pub entry: String,
}

#[allow(dead_code)]
impl RuleHarnessMeta {
    /// Derived, never stored.
    pub fn supports_user(&self) -> bool {
        self.dest.user.is_some()
    }

    /// Derived, never stored.
    pub fn supports_project(&self) -> bool {
        self.dest.project.is_some()
    }

    /// Installable iff a native renderer exists for `format` — i.e. the format
    /// is listed in `data/rule-formats.json`. Adding a harness with an
    /// unimplemented format must not make it installable.
    pub fn installable(&self) -> bool {
        crate::rules::registry::is_installable_with(
            &self.format,
            crate::rules::registry::implemented_formats(),
        )
    }
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- Agency Agents: Tool id ----------
    //
    // `Tool` is now a `String` camelCase id sourced from the embedded JSON
    // registry (which self-tests its id set in `crate::registry`). A tool id is
    // a plain string, so the former enum-variant serde tests are meaningless;
    // the wire-value coverage that still matters — that a `tool` field on a DTO
    // serializes as the exact camelCase string — lives in
    // `installed_agent_serializes_camel_case_fields` below.

    #[test]
    fn scope_and_states_serialize_camel_case() {
        assert_eq!(serde_json::to_string(&Scope::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&Scope::Project).unwrap(),
            "\"project\""
        );
        assert_eq!(
            serde_json::to_string(&InstallState::Foreign).unwrap(),
            "\"foreign\""
        );
        assert_eq!(
            serde_json::to_string(&UpdateKind::Substantive).unwrap(),
            "\"substantive\""
        );
    }

    #[test]
    fn installed_agent_serializes_camel_case_fields() {
        let a = InstalledAgent {
            slug: "frontend-developer".into(),
            name: "Frontend Developer".into(),
            tool: "claudeCode".to_string(),
            scope: Scope::User,
            project_path: None,
            dest: "/Users/x/.claude/agents/frontend-developer.md".into(),
            state: InstallState::Outdated,
            update_kind: Some(UpdateKind::Cosmetic),
            tracked: true,
        };
        let v = serde_json::to_value(&a).unwrap();
        for k in [
            "slug",
            "name",
            "tool",
            "scope",
            "projectPath",
            "dest",
            "state",
            "updateKind",
        ] {
            assert!(
                v.get(k).is_some(),
                "InstalledAgent must have wire field {:?}",
                k
            );
        }
        for snake in ["project_path", "update_kind"] {
            assert!(
                v.get(snake).is_none(),
                "snake key {:?} must not leak",
                snake
            );
        }
        assert_eq!(v["tool"], "claudeCode");
        assert_eq!(v["state"], "outdated");
        assert_eq!(v["updateKind"], "cosmetic");
    }

    #[test]
    fn corpus_entry_serializes_split_hashes_camel_case() {
        let e = CorpusEntry {
            slug: "code-reviewer".into(),
            name: "Code Reviewer".into(),
            category: "engineering".into(),
            emoji: Some("🔍".into()),
            color: None,
            vibe: None,
            description: "Reviews code.".into(),
            source_hash: "a".repeat(64),
            frontmatter_hash: "b".repeat(64),
            body_hash: "c".repeat(64),
        };
        let v = serde_json::to_value(&e).unwrap();
        for k in ["sourceHash", "frontmatterHash", "bodyHash"] {
            assert!(
                v.get(k).is_some(),
                "CorpusEntry must have wire field {:?}",
                k
            );
        }
        for snake in ["source_hash", "frontmatter_hash", "body_hash"] {
            assert!(
                v.get(snake).is_none(),
                "snake key {:?} must not leak",
                snake
            );
        }
    }

    // ---------- Rules (issue #29) ----------

    fn sample_rule() -> Rule {
        Rule {
            slug: "typescript".into(),
            name: "typescript".into(),
            description: "Mandatory rules for editing TypeScript and TSX files.".into(),
            category: "stacks".into(),
            relative_path: "rules/stacks/typescript.md".into(),
            globs: Some(RuleGlobs::Many(vec!["**/*.ts".into(), "**/*.tsx".into()])),
            always_apply: Some(false),
            trigger: Some("glob".into()),
            match_: Some("any".into()),
            tags: vec!["language".into()],
            tools: Vec::new(),
            body: "# TypeScript Rules".into(),
            source_hash: "a".repeat(64),
            body_hash: "b".repeat(64),
            validation: RuleValidation::passed(),
        }
    }

    #[test]
    fn rule_serializes_camel_case_and_keeps_relative_path() {
        let v = serde_json::to_value(sample_rule()).unwrap();
        assert_eq!(v["slug"], "typescript");
        assert_eq!(v["relativePath"], "rules/stacks/typescript.md");
        assert_eq!(v["alwaysApply"], false);
        assert_eq!(v["match"], "any");
        assert_eq!(v["sourceHash"], "a".repeat(64));
        assert!(v.get("match_").is_none(), "`match_` must not leak");
        assert!(v.get("relative_path").is_none());
        assert!(v.get("always_apply").is_none());
        assert!(v.get("validation").is_some());
    }

    /// C7: `Some(false)` and `None` are different rules. Absent must not emit
    /// the key at all; `false` must emit `false`.
    #[test]
    fn rule_distinguishes_absent_from_false_always_apply() {
        let mut absent = sample_rule();
        absent.always_apply = None;
        let v = serde_json::to_value(absent).unwrap();
        assert!(
            v.get("alwaysApply").is_none(),
            "absent alwaysApply must not emit a key, got {:?}",
            v.get("alwaysApply")
        );

        let v = serde_json::to_value(sample_rule()).unwrap();
        assert_eq!(v["alwaysApply"], false);
    }

    #[test]
    fn rule_globs_accepts_both_authored_shapes() {
        let one: Option<RuleGlobs> = serde_json::from_value(serde_json::json!("**/*.py")).unwrap();
        assert_eq!(one, Some(RuleGlobs::One("**/*.py".into())));

        let many: Option<RuleGlobs> =
            serde_json::from_value(serde_json::json!(["**/*.ts", "**/*.tsx"])).unwrap();
        assert_eq!(
            many,
            Some(RuleGlobs::Many(vec!["**/*.ts".into(), "**/*.tsx".into()]))
        );
    }

    #[test]
    fn rule_entry_carries_the_source_path_and_validation() {
        let entry = RuleEntry {
            slug: "commit-conventions".into(),
            name: "commit-conventions".into(),
            description: "Enforce Conventional Commits.".into(),
            category: "base".into(),
            relative_path: "rules/base/commit-convention.md".into(),
            globs: None,
            always_apply: Some(true),
            trigger: None,
            match_: None,
            tags: Vec::new(),
            tools: Vec::new(),
            source_hash: "a".repeat(64),
            body_hash: "b".repeat(64),
            validation: RuleValidation {
                valid: false,
                errors: vec!["boom".into()],
            },
        };
        let v = serde_json::to_value(&entry).unwrap();
        assert_eq!(v["relativePath"], "rules/base/commit-convention.md");
        assert_eq!(v["validation"]["valid"], false);
        assert_eq!(v["validation"]["errors"][0], "boom");
        // The index row is tailed by the list view, so an absent optional is an
        // explicit `null` here (the architecture's `RuleEntry` has no
        // `skip_serializing_if`), unlike `Rule`.
        assert!(v["globs"].is_null());
        assert_eq!(v["match"], serde_json::Value::Null);
    }
}
