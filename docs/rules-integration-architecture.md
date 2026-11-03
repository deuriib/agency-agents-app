# Rules Integration — Technical Architecture (Issue #29, Milestone M1)

> **Status**: Contract for implementation. Companion to
> `docs/rules-integration-spec.md` (approved) and
> `docs/rules-integration-tasks.md` (25 tasks + orchestrator rulings A1–A14).
> **Where this document and a task disagree, the orchestrator ruling wins, then
> the spec, then this document.** Every place I found the two source documents in
> conflict is listed in §11 — resolve those before writing code.
>
> **Audience**: the Rust and Svelte dev agents. Nothing here is optional
> decoration: each rule exists because guessing it produces a wrong byte or a
> lost ledger row.

---

## 1. Module map

New module tree, matching the `corpus/` precedent (commands live in their domain
module, not `src-tauri/src/commands/`) — ruling **A1**:

| Path | Responsibility (one line) |
| --- | --- |
| `src-tauri/src/rules/mod.rs` | Module surface only: `pub mod registry; parse; validate; render; corpus; opencode; commands;` + re-exports the command list `lib.rs` registers. No logic. |
| `src-tauri/src/rules/registry.rs` | Reads the two embedded contracts (`data/rules-tools.json`, `data/rule-formats.json`) via `include_str!`; harness lookup, deterministic ordering, derived `installable()` / `supports_user()` / `supports_project()`, and the `kebab → tool id` link from A7. |
| `src-tauri/src/rules/parse.rs` | Splits one `rules/<category>/<file>.md` into a typed `Rule`: frontmatter → fields, body → `body`, plus `source_hash` / `body_hash`. Pure, no I/O. |
| `src-tauri/src/rules/validate.rs` | Validates a parsed rule's frontmatter against the embedded `data/rules-schema.json` and returns `RuleValidation`. Pure, no I/O, no filesystem access. |
| `src-tauri/src/rules/render.rs` | The purpose-built YAML emitter, `wrap_rule`, the four renderers, `render_rule_files`, `rule_dests` (slug re-validation + `{{slug}}` substitution), and the harness base-path resolver. Pure except the resolver's settings read. |
| `src-tauri/src/rules/corpus.rs` | The `rules-corpus` root and `rules-index.json`: `resolve_rules_corpus`, `build_rules_index`, baseline seed, tarball refresh, `rules-meta.json`. Owns *all* rules filesystem roots. |
| `src-tauri/src/rules/opencode.rs` | The `opencode.json` plugin merge (ruling **A6**): a pure `plan_merge(existing_bytes, entry) -> MergePlan` plus an async applier. The only code allowed to touch `opencode.json`. |
| `src-tauri/src/rules/commands.rs` | The eight `#[tauri::command]` entry points; argument structs; orchestration order (validate → resolve → plan → write → ledger). |

**Why `opencode.rs` and not "put it in `commands.rs`"**: A6 adds a distinct
failure taxonomy (unreadable vs unparseable vs already-present) and a pure
planner that must be unit-tested with zero `AppHandle`. Folding it into
`commands.rs` would make that file the only untestable place in the module and
exceed the ~200-line budget for TASK 16 (`tasks.md:16-19`). Justification
recorded here per the reuse rule.

**No changes to existing modules beyond these exact touch points**:

| Existing file:line | Change |
| --- | --- |
| `src-tauri/src/lib.rs:13-22` | `mod rules;` |
| `src-tauri/src/lib.rs:147-206` | Eight `rules::commands::*` entries in `invoke_handler` |
| `src-tauri/src/types.rs:194-224` | `InstallDomain` + `InstallRecord.domain` (§4.6) |
| `src-tauri/src/install/mod.rs:102-112` | `resolve_tool_base` → `pub(crate)` (A7 reuses it, §9) |
| `src-tauri/src/install/mod.rs:836,1108,1118,1234,1281` | `domain == Agent` filters — see §11 conflict **C4**, these are *not* in the task list |
| `src-tauri/Cargo.toml:122` | `jsonschema` promoted to `[dependencies]` (A11) + `preserve_order` on `serde_json` (§8 step 6) |
| `src-tauri/tauri.conf.json:41-44` | `"resources/rules-baseline/**/*"` |

---

## 2. The declarative contract (two files, two readers)

Spec §5.5 and `registry.rs:19-38` establish the rule: *one file, embedded by Rust,
imported by TS*. Rules get the same pair, verbatim from the sibling repo:

### 2.1 `src-tauri/data/rules-tools.json`

```json
{
  "_note": "Rule harness manifest. The `harnesses` object is a verbatim copy of deuriib/agency-rules tools.json — do not rename keys, do not 'improve' values, do not add app-specific fields. Read by BOTH src-tauri/src/rules/registry.rs (include_str!) and src/lib/data/ruleRegistry.ts (JSON import). A harness is installable iff its `format` appears in data/rule-formats.json. Destinations use {{slug}} (DOUBLE braces) — the agent contract uses {slug}; do not unify them.",
  "version": 1,
  "harnesses": {
    "antigravity": { "…verbatim…": "…" },
    "pi-agent":    { "…verbatim…": "…" },
    "oh-my-pi":    { "…verbatim…": "…" },
    "opencode":    { "…verbatim…": "…" }
  }
}
```

The `harnesses` sub-object must deep-equal `harnesses` in
`/home/deuriib/projects/agency-rules/tools.json`. The values a dev is most likely
to mis-transcribe — treat this table as the byte contract:

| key | `id` | `label` | `format` | `installKind` | `integrationPath` | `dest.project` | `dest.user` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `antigravity` | `antigravity` | `Google Antigravity` | `md-yaml-trigger` | `per-rule` | `{{slug}}.md` | `.agents/rules/{{slug}}.md` | `~/.gemini/config/rules/{{slug}}.md` |
| `pi-agent` | `pi-agent` | `Pi Coding Agent` | `md-rules-dir` | `per-rule` | `{{slug}}/RULES.md` | `.pi/rules/{{slug}}/RULES.md` | `~/.pi/agent/rules/{{slug}}/RULES.md` |
| `oh-my-pi` | `oh-my-pi` | `oh-my-pi (omp)` | `md-or-mdc` | `per-rule` | `{{slug}}.md` | `.omp/rules/{{slug}}.md` | `~/.omp/agent/rules/{{slug}}.md` |
| `opencode` | `opencode` | `OpenCode (via opencode-rules)` | `md-plugin` | `per-rule` | `{{slug}}.md` | `.opencode/rules/{{slug}}.md` | `~/.config/opencode/rules/{{slug}}.md` |

Three blocks beyond that table must survive the copy, because three code paths
read them:

- `detect` — documentation only for M1; parse it (`#[serde(default)]`) so the
  file stays verbatim, but nothing consumes it.
- `frontmatter` — documentation only for M1 (the renderers hard-code their field
  sets because byte parity, not the map, is the contract). Parse-and-ignore.
- **`opencode.plugin`** = `{"package": "opencode-rules", "entry": "opencode-rules@latest"}` —
  **consumed by `rules/opencode.rs`**. A6 forbids hardcoding either value.
- `oh-my-pi.extensions: [".md", ".mdc"]` — parse-and-ignore; the renderer emits
  `.md` and the `.md` is carried by the dest template.

### 2.2 `src-tauri/data/rule-formats.json`

Mirrors `render-formats.json` exactly (`src-tauri/data/render-formats.json`):

```json
{
  "_note": "The rule render formats this app implements natively. A harness is installable iff its rules-tools.json `format` appears in this list. Read by BOTH src-tauri/src/rules/registry.rs (include_str!) and src/lib/data/ruleRegistry.ts (JSON import) — never restate this list in code.",
  "formats": ["md-or-mdc", "md-plugin", "md-rules-dir", "md-yaml-trigger"]
}
```

### 2.3 The two-sides-cannot-drift guard (name it exactly this)

| Side | File | What it asserts |
| --- | --- | --- |
| Rust | `src-tauri/src/rules/registry.rs`, test `contracts_are_single_sourced_and_resolvable` | The two `include_str!` constants are the only source of harness/format data (assert no `"md-"` literal appears in `registry.rs` outside the `include_str!` lines — same technique as `registry.rs:242-260`); every harness `format` resolves; a harness injected with an unknown format is not installable (parse a literal JSON string in the test, never edit the bundled file). |
| TS/Node | `scripts/check-rule-registry.test.mjs`, test `"rust and ts derive the same harness set"` | `singleSourceWiring(root)` is `{ rust: true, frontend: true }`; the four harness ids parsed from `rules/registry.rs`-adjacent RSSource equal the ids in `ruleRegistry.ts`'s import result as sorted arrays; `formatCoverage` has zero orphans. |
| Corpus parity | `scripts/check-rule-registry.test.mjs`, test `"bundled harnesses are verbatim upstream"` | `rules-tools.json`.harnesses deep-equals `agency-rules/tools.json`.harnesses **when `AGENCY_RULES_ROOT` is set** (skip cleanly when unset — CI has no sibling checkout, exactly like the `AGENCY_RULES_PARITY_ROOT` pattern in TASK 10). |

The Node guard is the one TASK 2 specifies (`scripts/check-rule-registry.mjs` +
`.test.mjs`, run by `npm run test:scripts`).

### 2.4 Why `{{slug}}` and not `{slug}`

The agent contract uses single braces (`src-tauri/data/tools.json`, substituted
at `render/mod.rs:566`). The rules contract uses double (ruling **A2**). Two
different placeholder dialects in the same binary is a footgun, so
`rules/render.rs` gets its own substitution function and its own guard:
`assert!(!resolved.to_string_lossy().contains('{'))` after substitution. A test
asserts `rule_dests` rejects a template containing `{slug}` with
`AppError::InvalidArgument` rather than writing a literal `{slug}` directory.

---

## 3. Rust types — signatures only

All in `src-tauri/src/types.rs` unless noted, so the wire contract lives in one
place (matching `contracts.md` §A). Every struct carries
`#[serde(rename_all = "camelCase")]`.

### 3.1 `RuleHarness` and the parsed rule

```rust
/// A rule harness id, e.g. "oh-my-pi". A plain string (not an enum) for the same
/// reason `Tool` became a String (types.rs:16-22): the set lives in embedded
/// JSON, so adding a harness is a JSON edit, not a Rust variant.
pub type RuleHarness = String;

/// Authored `globs` in the neutral format accepts BOTH shapes
/// (agency-rules/rules/_schema.json uses oneOf). Untagged preserves which form
/// was authored — oh-my-pi and opencode must re-emit the authored shape
/// (tasks.md TASK 7/8), antigravity always joins with ','.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleGlobs {
    One(String),
    Many(Vec<String>),
}
```

```rust
/// A fully parsed neutral rule (`rules/<category>/<file>.md`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// Frontmatter `name`, NOT the filename (ruling A3;
    /// rules/base/commit-convention.md declares `name: commit-conventions`).
    pub slug: String,
    /// Frontmatter `name` again. Equal to `slug` by construction (A3) — both are
    /// required by spec §5.1, and keeping the display name separate means the
    /// renderers read one field for output and the map key stays the identity.
    pub name: String,
    pub description: String,
    /// Parent directory name, e.g. "base" / "stacks".
    pub category: String,
    /// Corpus-relative source path, e.g. "rules/base/commit-convention.md".
    /// **Required** — slug != filename, so this cannot be re-derived.
    pub relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub globs: Option<RuleGlobs>,
    /// Option<bool>, never bool: `alwaysApply: false` must be distinguishable
    /// from absent (TASK 7 is keyed on presence, not truthiness).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_apply: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// `match` is a Rust keyword. The serde rename is mandatory; forgetting it
    /// silently drops the field to `None` and the OpenCode renderer loses output.
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
```

```rust
/// `rules_list` row: identical to `Rule` minus `body` (mirrors `corpus_list`,
/// which omits the body to keep the payload small — `corpus/mod.rs:1837-1846`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSummary {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub relative_path: String,
    pub globs: Option<RuleGlobs>,
    pub always_apply: Option<bool>,
    pub trigger: Option<String>,
    #[serde(rename = "match")]
    pub match_: Option<String>,
    pub tags: Vec<String>,
    pub tools: Vec<String>,
    pub validation: RuleValidation,
}
```

Serialization note: `#[serde(default)]` on the *deserialize* side plus
`skip_serializing_if` on the outbound side means `alwaysApply: false` travels as
`"alwaysApply": false` and absent travels as no key. TypeScript must therefore
model `alwaysApply?: boolean` (`boolean | undefined`), never `boolean`.

### 3.2 The index row

```rust
/// One row of `<app_data>/state/rules-index.json`. Carries everything the list
/// view and every renderer need, so a list render is zero filesystem reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEntry {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    /// See Rule::relative_path — the index is the only place the mapping
    /// slug → file survives. Dropping it makes `rules_get` impossible.
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
    /// explain why install is refused (TASK 11; spec §10.4).
    pub validation: RuleValidation,
}
```

Persisted shape (deterministic — no timestamps, `BTreeMap` for stable key order,
same contract as `corpus-index.json` per `systemPatterns.md` §2):

```json
{
  "count": 8,
  "rules": { "<slug>": { "…RuleEntry…": "…" } }
}
```

### 3.3 The harness metadata (what the registry returns)

```rust
/// One harness, as authored in data/rules-tools.json. Mirrors the
/// registry.rs:82-147 pattern (`ToolMeta` + derived capabilities).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleHarnessMeta {
    pub id: String,
    pub label: String,
    pub format: String,
    pub install_kind: String,
    pub integration_path: String,
    #[serde(default)]
    pub detect: Option<serde_json::Value>,       // parsed, unused (M1)
    #[serde(default)]
    pub frontmatter: Option<serde_json::Value>,  // parsed, unused (M1)
    #[serde(default)]
    pub extensions: Vec<String>,                 // parsed, unused (M1)
    #[serde(default)]
    pub plugin: Option<RulePlugin>,
    pub dest: RuleHarnessDest,
}

/// NOTE the shape difference from `registry::Dest` (registry.rs:69-75): rule
/// destinations are a SINGLE optional template per scope, not a Vec.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuleHarnessDest {
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RulePlugin {
    pub package: String,
    pub entry: String,
}

impl RuleHarnessMeta {
    /// Derived, never stored (spec §5.3).
    pub fn supports_user(&self) -> bool;      // self.dest.user.is_some()
    pub fn supports_project(&self) -> bool;   // self.dest.project.is_some()
    /// Installable iff a native renderer exists for `format` (rule-formats.json).
    pub fn installable(&self) -> bool;        // implemented_formats().contains(&self.format)
}
```

Registry surface, mirroring `registry.rs:177-189`:

```rust
pub fn all() -> &'static [RuleHarnessMeta];            // sorted by (label, id)
pub fn get(id: &str) -> Option<&'static RuleHarnessMeta>;
pub fn installable() -> impl Iterator<Item = &'static RuleHarnessMeta>;
pub fn implemented_formats() -> &'static HashSet<String>;   // include_str! only
pub fn plugin_entry(id: &str) -> Option<&'static str>;      // A6: never hardcode
```

### 3.4 Validation, preview, status

```rust
/// Pure result of rules/validate.rs. `errors` are the schema validator's own
/// messages, unprefixed and untruncated ("surfaced verbatim", spec §8).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}
```

```rust
/// `rules_preview` response. One entry per INSTALLABLE harness (spec §8:
/// "Unknown harness → never offered"). Nothing is written; `dest` is computed
/// and never created.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePreview {
    pub slug: String,
    /// Which scope was actually resolved, so the UI cannot show a user path
    /// while the user has a project selected (ruling A9).
    pub scope: Scope,
    pub project_path: Option<String>,
    pub harnesses: Vec<RulePreviewEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePreviewEntry {
    /// The harness id from the contract — a String, not the agent `Tool`.
    pub harness: String,
    pub label: String,
    /// Exact bytes that `rules_install` would write (proven equal by test).
    pub bytes: String,
    /// Absolute destination path.
    pub dest: String,
    pub rendered_hash: String,   // render::sha256_hex(bytes)
    pub installable: bool,       // always true in M1; kept so the UI has a
                                 // field to branch on if a format ships later
}
```

```rust
/// `rules_status` — the only source of the "not synced" vs "no rules shipped"
/// distinction (spec §8, ruling A5).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleStatus {
    /// The rules corpus root exists and is non-empty.
    pub configured: bool,
    pub count: u32,
    /// From rules-meta.json; "baseline" before the first refresh.
    pub version: String,
    /// null when there are rules to show.
    pub empty_reason: Option<RuleEmptyReason>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuleEmptyReason {
    /// Corpus root missing/empty — first launch before seeding finished, or a
    /// failed seed. CTA: sync.
    NotSynced,
    /// Corpus root present, `rules/` scanned, zero valid files. CTA: refresh,
    /// and say so honestly.
    NoRules,
}
```

### 3.5 Harness → tool link (A7)

```rust
/// Resolved by `kebab` lookup in the EXISTING agent registry — not a second
/// table (ruling A7). Every shipped harness id equals a `kebab` in
/// data/tools.json: antigravity→antigravity, pi-agent→piAgent,
/// oh-my-pi→ohMyPi, opencode→opencode.
pub fn tool_for_harness(harness: &str) -> Option<&'static crate::registry::ToolMeta>;
```

### 3.6 The ledger row, and how `domain` lands without moving agent bytes

The requirement (spec §5.4): a v0.8.x ledger still parses, agent rows re-serialize
**byte-identically**, rule rows are distinguishable, and rules never surface in
agent reconciliation.

```rust
/// Which domain a ledger row belongs to. Default = Agent so a missing field
/// (every row written before M1) parses as an agent row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallDomain {
    #[default]
    Agent,
    Rule,
}

/// Serde predicate for skip_serializing_if. MUST be a free function: serde
/// resolves the path, so a `Self::` method does not work here.
fn is_agent_domain(d: &InstallDomain) -> bool {
    matches!(d, InstallDomain::Agent)
}
```

The field, appended **after** `corpus_version` so the struct declaration order
(which is the serialization order for agent rows too) is unchanged for every
existing key:

```rust
pub struct InstallRecord {          // types.rs:194-224 — one field appended
    // … slug, tool, scope, project_path, dest, dests, file_hashes,
    //    aggregate_slugs, source_hash, body_hash, rendered_hash,
    //    installed_at, corpus_version — ALL UNCHANGED …
    /// Domain discriminator. Added in M1. `#[serde(default)]` keeps old
    /// ledgers parsing; `skip_serializing_if` keeps agent rows byte-identical
    /// (without it every agent row gains `"domain": "agent"` and TASK 13's
    /// regression gate fails).
    #[serde(default, skip_serializing_if = "is_agent_domain")]
    pub domain: InstallDomain,
}
```

Re-serialization proof (this is the regression gate, not a smoke test):

```rust
#[test]
fn v0_8_x_ledger_round_trips_byte_identically() {
    let raw = include_bytes!("../tests/fixtures/installs-v0.8.x.json");
    let mut recs: Vec<InstallRecord> = serde_json::from_slice(raw).unwrap();
    for r in &mut recs { if r.dests.is_empty() { r.dests = vec![r.dest.clone()]; } }
    let out = serde_json::to_vec_pretty(&recs).unwrap();
    assert_eq!(out, raw, "agent rows must serialize byte-identically");
}
```

Rule rows reuse the existing string fields instead of parallel pairs:
`slug` = rule slug, `tool` = **harness id** (`"oh-my-pi"`, kebab),
`scope` = `render::scope_for(project_path)` (`render/mod.rs:53-59`),
`dest` = primary written path, `dests` = the rule render file(s) **only** (see
§11 conflict **C2** about `opencode.json`).

TypeScript mirror (`src/lib/types.ts:524-537` today):

```ts
export type InstallDomain = "agent" | "rule";

export interface InstallRecord {
  slug: string;
  tool: Tool;                 // agent tool id OR rule harness id
  scope: Scope;
  projectPath: string | null;
  dest: string;
  dests?: string[];           // present on rows this build writes
  sourceHash: string;
  bodyHash: string;
  renderedHash: string;
  installedAt: string;
  corpusVersion: string;
  /** Absent on agent rows — the backend skips the key entirely. */
  domain?: InstallDomain;
}

/** A ledger row this app wrote for a rule. Narrows `tool` to a harness id. */
export type RuleInstall = InstallRecord & { domain: "rule" };
```

---

## 4. Command surface

`src-tauri/src/rules/commands.rs`, all registered in `lib.rs`'s
`invoke_handler` (`lib.rs:147-206`) fully qualified, alongside
`corpus::*` / `install::*`.

| Command | Signature | async | `AppHandle` | `State` | Returns |
| --- | --- | --- | --- | --- | --- |
| `rules_list` | `(app: AppHandle, state: State<'_, AppState>, category: Option<String>) -> Result<Vec<RuleSummary>, AppError>` | yes | yes | yes | `RuleSummary[]` |
| `rules_get` | `(app: AppHandle, state: State<'_, AppState>, slug: String) -> Result<Rule, AppError>` | yes | yes | yes | `Rule` |
| `rules_preview` | `(app: AppHandle, state: State<'_, AppState>, slug: String, project_path: Option<String>) -> Result<RulePreview, AppError>` | yes | yes | yes | `RulePreview` |
| `rules_install` | `(app: AppHandle, state: State<'_, AppState>, slug: String, harness: String, project_path: Option<String>) -> Result<InstallRecord, AppError>` | yes | yes | yes | `InstallRecord` (TS: `RuleInstall`) |
| `rules_uninstall` | `(app: AppHandle, state: State<'_, AppState>, slug: String, harness: String, project_path: Option<String>) -> Result<(), AppError>` | yes | yes | yes | `()` |
| `rules_status` | `(app: AppHandle) -> Result<RuleStatus, AppError>` | yes | yes | no | `RuleStatus` |
| `rules_installs` | `(app: AppHandle) -> Result<Vec<InstallRecord>, AppError>` | yes | yes | no | `InstallRecord[]` (TS: `RuleInstall[]`) |
| `rules_refresh` | `(app: AppHandle, state: State<'_, AppState>) -> Result<RuleStatus, AppError>` | yes | yes | yes | `RuleStatus` |

Why every one is `async`: they all reach `tokio::fs`. Why none take
`State` except where settings or the network gate is needed: `rules_status` and
`rules_installs` are pure reads of app-data paths, and `rules_installs` must be
answerable after a restart with no corpus involvement (spec §10.6).

**`rules_uninstall` argument-shape note**: the `harness` argument is why the
frontend must send the harness id, never the label. `ruleRegistry.ts` exposes
`id`; the UI displays `label`.

**`rules_install` fixed order (fail-closed, TASK 16) — no step may be reordered:**

```
1. ensure_rules_corpus → find RuleEntry by slug              (unknown → InvalidArgument)
2. validate slug regex ^[a-z][a-z0-9-]*$                     (→ InvalidArgument)
3. validate frontmatter                                      (→ InvalidArgument, verbatim)
4. registry::get(harness)                                    (unknown → InvalidArgument)
5. harness.installable()                                     (→ InvalidArgument)
6. project scope && project_path.is_none()                   (→ InvalidArgument)
7. render_rule_files + rule_dests (slug re-validated again)   (§7)
8. opencode::plan_merge  ← PURE, no write (harness == "opencode" only)  (§8)
9. opencode::apply(plan)        ← first write
10. atomic_write(rule dests)    ← second write(s)
11. ledger retain-then-push + save
```

Steps 9 before 10 is deliberate: if the rule file landed but the plugin write
failed, the rule would be inert with no error path. This order means a failure at
step 10 leaves a plugin entry and no rule — harmless (the plugin does nothing
without rules) and fixed by a retry, because the merge is idempotent.

---

## 5. YAML emitter contract (Antigravity + the three other frontmatter blocks)

> This section exists because the reference emits bytes `serde_yaml` cannot
> produce (ruling **A13**) and because a single wrong quote breaks
> `rendered_hash`. Nothing below is negotiable; every clause was measured against
> `yaml@2` (the package `agency-rules` uses) at
> `/home/deuriib/projects/agency-rules/node_modules/yaml`.

### 5.1 The value model

```rust
/// The only three value kinds any renderer emits. Deliberately tiny: the field
/// set is fixed and closed (ENG-13 — no abstraction without a pain point).
pub enum YamlValue {
    Scalar(String),
    Bool(bool),
    Seq(Vec<String>),
}

/// Emit one ordered field list as a YAML block (no `---` fences, no trailing
/// newline). Caller order is the emission order — there is no sorting.
pub fn emit_block(fields: &[(&str, YamlValue)]) -> String;
```

### 5.2 Field order per harness (fixed; the caller supplies it in this order)

| Harness | Order, with the exact inclusion predicate |
| --- | --- |
| antigravity | `trigger` (always) → `globs` **only if** `trigger == "glob"` **and** globs present, `RuleGlobs` joined with `,` and no space → `description` **only if** non-empty after trim |
| pi-agent | `name` → `description` → `tools` **only if** `tools.len() > 0` |
| oh-my-pi | `name` → `description` → `globs` **if present** → `alwaysApply` **if present** (presence, not truthiness — `false` MUST be emitted) |
| opencode | `description` → `globs` **if present** → `match` **if present** (`name`, `alwaysApply`, `trigger`, `tags`, `tools` never emitted) |

Antigravity `trigger` precedence, evaluated in this order (five branches +
explicit override; one test each):

```
explicit `trigger` present            → use it verbatim
else `alwaysApply == true`            → "always_on"
else `globs` present (string or arr)  → "glob"
else `description` non-empty          → "model_decision"
else                                  → "manual"
```

### 5.3 Scalar emission

For a `Scalar` in **value position** (`key: <scalar>`) the emitter decides
plain vs quoted. Measured behaviour of `yaml@2` — implement exactly this and
nothing "safer":

| Input | Emitted |
| --- | --- |
| `glob`, `always_on`, `model_decision`, `manual`, `any`, `all` | plain |
| `plain text`, `a b  c`, `star*`, `100%`, `a#b`, `key:value`, `=eq`, `2026-11-03`, `yes`, `no`, `on`, `off`, `?q` | plain |
| `**/*.ts`, `*.md`, `*star` | `"**/*.ts"` — leading `*` forces double quotes |
| `has: colon`, `a: b`, `ends with colon:` | `"…"` — `": "` anywhere, or trailing `:` |
| `#hash`, `trail hash #` | `"…"` — leading `#`, or ` #` anywhere |
| ` leading`, `trailing `, `  ` | `"…"` — leading/trailing space |
| `- dash`, `? q`, `%p`, `!bang`, `&amp`, `\|pipe`, `>gt`, `[a]`, `{a}`, `@at`, `` `tick ``, `"quoted"`, `'single'` | `"…"` — leading indicator character |
| `""` (empty) | `""` |
| `true`, `false`, `null`, `~`, `123`, `1.5`, `1e9`, `0x1f` | `"…"` — would parse as YAML 1.2 core bool/null/number |
| value contains `"` but not `'` | single-quoted (`'"quoted"'`) |
| value contains both quote kinds, no other trigger | **plain** |

Two traps worth naming in a code comment:

1. `yes` / `no` / `on` / `off` are **not** quoted (YAML 1.2 core, not 1.1).
   Quoting them would diverge.
2. `key:value` (colon with no space) is **not** quoted; only `": "` and a
   trailing `:` trigger quoting.

For a `Scalar` in **sequence position** (`  - <scalar>`) the same predicate
applies, and the measured outcome for the corpus's real globs is:

```
globs:
  - "**/*.ts"        ← leading '*' → double-quoted
  - src/**           ← no leading indicator → PLAIN, unquoted
```

A naive "always quote globs" emitter passes all 28 fixtures today and would
diverge the first time a rule authors `globs: src/**`. Use the predicate.

For `Bool`: emit `true` / `false`, never quoted.

### 5.4 Line folding — the highest-risk clause

`yaml@2` wraps a plain or double-quoted scalar at **80 columns**, greedy, at
spaces, with a **2-space continuation indent**. The limit counts the whole line
including the `key: ` prefix on the first line and the 2 spaces on continuations.
Lines may be **exactly 80**; a word that would make 81 moves to the next line.

Measured, and the reason this clause is mandatory:

```
description: Apply these guidelines for Nexus Strategy activation, runbooks,
  playbooks, and coordination.
```

The folding applies to plain **and** double-quoted scalars (the closing `"` sits
after the last word of the last wrapped line). It never breaks inside a word and
never applies to sequence items.

This is exercised by exactly **4 of the 28** conformance comparisons
(`nexus-strategy` × 4 harnesses). Everything else in the corpus is under the
fold. That means:

- A folding bug is invisible to 24/28 comparisons. **The emitter's own unit
  tests are the real gate**, not the fixtures.
- `cargo test --lib rules::render::yaml` must include:
  `yaml_folds_at_80_columns` (a 130-char description → exact expected bytes),
  `yaml_does_not_fold_at_80` (a line landing on exactly 80 stays on one line),
  `yaml_does_not_break_long_words`, and `yaml_scalar_quoting_table`
  (table-driven over every row of §5.3).

### 5.5 `wrap_rule` — the byte template

```rust
/// `---\n<yaml>\n---\n\n<trimmed body>\n`
///
/// NOTE the BLANK LINE after the closing fence. The agent renderers
/// (render/mod.rs:390-506) emit `---\n…\n---\n{body}\n` with no blank line.
/// Do NOT reuse that template — it fails all 28 conformance comparisons.
pub fn wrap_rule(fields: &[(&str, YamlValue)], body: &str) -> String {
    // let yaml = emit_block(fields);            // no trailing newline
    // let content = body.trim();
    // format!("---\n{yaml}\n---\n\n{content}\n")
    todo!("signature only")
}
```

Both trims are load-bearing: `emit_block` must not end with a newline (the
reference does `.trimEnd()`), and the body is `trim()`-ed before the hash is
computed (TASK 3).

---

## 6. Renderers and destinations

`src-tauri/src/rules/corpus.rs` surface (the layer every command reads through —
mirrors `corpus::ensure_corpus` / `Corpus` at `corpus/mod.rs:191-260`):

```rust
/// Resolved, in-memory rules corpus: the index plus the paths to read bodies from.
pub struct RulesCorpus { /* index: BTreeMap<String, RuleEntry>, root, version */ }

impl RulesCorpus {
    pub fn get(&self, slug: &str) -> Option<RuleEntry>;
    pub fn entry(&self, slug: &str) -> Option<&RuleEntry>;
    pub fn list(&self, category: Option<&str>) -> Vec<RuleEntry>;
    pub fn version(&self) -> &str;
    pub fn len(&self) -> usize;
    pub fn read_body(&self, entry: &RuleEntry) -> Result<String, AppError>; // async
}

/// Seed on first use, build the index, cache it on `AppState`.
pub async fn ensure_rules_corpus(
    app: &AppHandle,
    state: &AppState,
) -> Result<std::sync::Arc<RulesCorpus>, AppError>;

/// Distinct, non-overlapping roots (TASK 25 asserts this):
pub fn rules_corpus_dir(app_data_dir: &Path) -> PathBuf;   // <app_data>/rules-corpus
pub fn rules_index_path(app_data_dir: &Path) -> PathBuf;   // <app_data>/state/rules-index.json
pub fn rules_meta_path(app_data_dir: &Path) -> PathBuf;    // <app_data>/state/rules-meta.json
```

```rust
/// One dispatch point, mirroring render::render_files (render/mod.rs:328-368).
/// Returns exactly one RenderedFile for all four formats; `relative` stays ""
/// because the dest template already carries the nesting ({{slug}}/RULES.md).
pub fn render_rule_files(
    rule: &Rule,
    harness: &str,
) -> Result<Vec<crate::render::RenderedFile>, AppError>;

/// Absolute destination(s) for (harness, slug, scope).
/// `base` is the USER base resolved by §9 (A7) — NOT the raw OS home.
pub fn rule_dests(
    harness: &str,
    slug: &str,
    base: &Path,
    project_root: Option<&Path>,
) -> Result<Vec<PathBuf>, AppError>;

/// `^[a-z][a-z0-9-]*$` — the schema pattern from rules/_schema.json, re-checked
/// in Rust before any path join (spec §8, DENY-03). One function, called at the
/// dest resolver AND again at the write boundary in `rules_install`.
pub fn validate_slug(slug: &str) -> Result<(), AppError>;
```

`rule_dests` algorithm:

```
1. validate_slug(slug)?                                  // nothing is joined yet
2. meta = registry::get(harness).ok_or(unknown harness)?
3. template = match project_root {
       Some(_) => meta.dest.project,                    // None → project-scoped error
       None    => meta.dest.user,                        // None → user-scoped error
   }
4. reject template if Path::new(template).is_absolute()  // DENY-03; InvalidArgument
5. stripped = template.strip_prefix("~/").unwrap_or(template)
6. rendered = stripped.replace("{{slug}}", slug)
7. assert no '{' remains → else InvalidArgument "template uses {slug}; the rule
   contract uses {{slug}}"
8. root = project_root.map(PathBuf::from).unwrap_or(base)
9. return vec![root.join(rendered)]
```

The "project scope without a root" error reuses the agent wording
(`render/mod.rs:556-561`):
`rule harness '{harness}' is project-scoped; a project path is required`.
All four shipped harnesses support both scopes, so this branch is reached only by
a caller passing garbage — which is exactly what the test asserts.

---

## 7. Byte-equality proof

| Layer | Test | Proves |
| --- | --- | --- |
| Emitter | `cargo test --lib rules::render::yaml` | quoting table, folding at 80, long-word behaviour, `wrap_rule` shape |
| Precedence | `cargo test --lib rules::render::antigravity` | five branches + explicit-trigger override, both `globs` shapes, comma join with no space |
| Dropped fields | `cargo test --lib rules::render::{pi_agent,oh_my_pi,opencode}` | absent-substring assertions for every dropped key; `alwaysApply: false` present |
| **Conformance** | `cargo test --test rules_conformance` → `renders_all_fixtures_byte_identically` | 7 sources × 4 harnesses = **28** `Vec<u8>` comparisons of `src-tauri/tests/fixtures/rules/**` |
| Preview == install | `cargo test --lib rules::commands` | preview bytes and `rendered_hash` equal the installed file's bytes |
| Staleness | `#[ignore]` `rules_parity_against_checkout`, gated on `AGENCY_RULES_PARITY_ROOT` | re-renders the live sibling checkout's `rules/` and diffs `integrations/` (A14; documented in `docs/BUILD.md` beside `mise run parity`) |

The conformance test must compare `Vec<u8>` (`.as_bytes()`), not `String`, and on
failure print the first-divergence offset with the expected/actual byte — TASK 10
requires it and it is the difference between a 5-minute fix and a 2-hour one.

**Read §11 conflict C5 before writing the fixture list**: the corpus has 8 rules
but `integrations/` only carries 7.

---

## 8. `opencode.json` merge algorithm (A6)

Target file:

| Scope | Path |
| --- | --- |
| project | `<project_root>/opencode.json` |
| user | `<user_base_for_harness("opencode")>/.config/opencode/opencode.json` |

where `<user_base_for_harness>` is §9's resolver — so a `settings.tool_paths`
override for the `opencode` tool moves the plugin config too, which is the point
of A7.

```rust
pub enum MergeAction {
    /// The file already registers a matching plugin entry — write nothing.
    NoOp { path: PathBuf },
    /// Create or extend the document.
    Write { path: PathBuf, bytes: Vec<u8> },
}

/// Pure: reads only `existing` (None = file absent) and the contract's entry.
/// Returns Err on any condition that must abort the install.
pub fn plan_merge(
    path: PathBuf,
    existing: Option<&[u8]>,
    entry: &str,
) -> Result<MergeAction, AppError>;

pub async fn apply(action: &MergeAction) -> Result<(), AppError>;  // atomic_write
```

Numbered algorithm, with every failure branch:

1. `entry = registry::plugin_entry("opencode")`. `None` → `AppError::Internal`
   ("rules-tools.json: opencode has no plugin.entry") — a build-time authoring
   error, never hardcoded (A6).
2. Read the target path.
   - `ErrorKind::NotFound` → `MergeAction::Write { bytes: pretty({"plugin":[entry]}) }`.
   - Any other I/O error → `AppError::Io { message: format!("read {path}: {e}") }` and **no write**.
3. Parse the bytes as `serde_json::Value`.
   - Parse error → `AppError::Io { message: format!("parse {path}: {e}") }` and **no write**.
   - Not a JSON object (`Value::Array`/`String`/…) → `AppError::InvalidArgument
     { message: format!("{path} is not a JSON object; refusing to modify it") }` and **no write**.
4. Inspect `doc["plugin"]`:
   - **absent** → insert `doc["plugin"] = [entry]`, go to 6.
   - **present, not an array** → `AppError::InvalidArgument { message:
     format!("\"plugin\" in {path} is not an array; refusing to modify it") }`,
     **no write**. Never coerce, never overwrite.
5. Scan the array for a match: any element that is a `Value::String` whose text
   `.contains("opencode-rules")` → `MergeAction::NoOp`, **write nothing** (not an
   empty rewrite; the file's mtime must not change). Otherwise push `entry`.
   Non-string elements are ignored for matching and preserved untouched.
6. Serialize `doc` with `serde_json::to_string_pretty` (**2-space indent**) plus
   a trailing `\n`.
7. `apply` → `crate::util::fs::atomic_write(&path, &bytes)` (`util/fs.rs:33`).
   Never `std::fs::write`. Failure → `AppError::Io` naming the path and OS error;
   the install aborts before the rule file is written.

**Key-order preservation (a required Cargo change).** `serde_json::Value` uses a
`BTreeMap` by default, so a round-trip re-alphabetizes `opencode.json` —
violating A6's "never reorder unrelated keys". Enable the feature:

```toml
serde_json = { version = "1", features = ["preserve_order"] }
```

Blast radius, verified: the feature changes only `serde_json::Map` ordering, i.e.
`Value` round-trips. Every persisted artifact this app writes —
`corpus-index.json`, `rules-index.json`, `installs.json`, the Agentfile — is
serialized from **structs**, where field order is declaration order, so bytes are
unaffected. TASK 25's old-ledger byte-equality test is the guard that proves it.

**Indentation is normalized, not detected.** A user file indented with 4 spaces or
tabs comes back with 2. That is a rewrite of whitespace, not of keys, and it is
documented here as accepted. Do not attempt to sniff the original indent: there is
no reliable way to do it and an inconsistent indent is not a correctness property.

---

## 9. Harness → tool resolution (A7)

```rust
/// The USER-SCOPE base for a rule harness.
/// Total: no panic, no error, even for a harness with no registry match.
pub fn harness_base(
    tool_paths: &std::collections::HashMap<String, String>,
    harness: &str,
    os_home: &Path,
) -> PathBuf {
    match tool_for_harness(harness) {
        // No kebab match (a future harness, or a typo) → OS home. Unreachable
        // for the four shipped harnesses; covered by a synthetic test so it is
        // not dead code (ENG-07).
        None => os_home.to_path_buf(),
        // Reuse the ONE implementation of "custom path wins, else home"
        // (install/mod.rs:102-112, promoted to pub(crate) — do not copy it).
        Some(tool) => crate::install::resolve_tool_base(tool_paths, &tool.id, os_home),
    }
}
```

Resolution order, in words:

1. `harness` → `ToolMeta` by `kebab == harness` over `registry::all()`.
   The four shipped mappings are `antigravity → antigravity`,
   `pi-agent → piAgent`, `oh-my-pi → ohMyPi`, `opencode → opencode`.
2. `settings.tool_paths[tool.id]`, if present and non-empty → that directory.
3. Otherwise the OS home (`dirs::home_dir()`, via `install::home()` pattern at
   `install/mod.rs:76-80`).
4. Project scope never consults any of the above — it roots at the picked project.

**Regression guard (TASK 25).** `harness_base` must not change a single resolved
*agent* path. The test asserts, for each of the four harnesses, that
`harness_base(empty, h, home) == home`, and separately that
`render::dests("opencode" | "antigravity" | "piAgent" | "ohMyPi", …)` is
unchanged by the presence of a `tool_paths` entry — because those go through
`registry::Dest`, not through `harness_base`.

Note the deliberate consequence: a `tool_paths` override for `opencode` now
affects **both** agent installs (`~/.config/opencode/agents/`) and rule installs
(`~/.config/opencode/rules/`) and the plugin config
(`~/.config/opencode/opencode.json`). That is correct — they are the same tool's
config tree — and should be said once in the confirmation copy for project/user
scope so the user is not surprised.

---

## 10. Error taxonomy

Rules paths reuse the existing `AppError` (`src-tauri/src/error.rs:15-119`).
**No new variant is added** in M1: the TS side consumes a closed union
(`src/lib/types.ts:291-307`) and `appErrorMessage` (`types.ts:336`) already
renders every arm. The discriminator is the message, and the message always names
the path or the slug.

| Failure class | Variant | Exact message shape |
| --- | --- | --- |
| Unknown rule slug (`rules_get` / preview / install) | `InvalidArgument` | `unknown rule: {slug}` (mirrors `corpus_get`) |
| Slug fails `^[a-z][a-z0-9-]*$` | `InvalidArgument` | `invalid rule slug: {slug:?}` |
| Unknown harness | `InvalidArgument` | `unknown rule harness: {harness}` |
| Harness has no renderer | `InvalidArgument` | `rule harness '{harness}' has no renderer in this app (format {format})` |
| Frontmatter invalid | `InvalidArgument` | the schema messages, joined with `\n`, each unprefixed and untruncated (spec §8 "surfaced verbatim") |
| Project scope, no root | `InvalidArgument` | `rule harness '{harness}' is project-scoped; a project path is required` |
| Duplicate slug across categories | `InvalidArgument` | `duplicate rule slug '{slug}' in {path_a} and {path_b}` |
| Rules corpus root missing / unreadable | `Io` | `read {path}: {e}` |
| `rules/` dir absent inside an existing root | `Io` | `{root} has no rules/ directory` |
| Unreadable rule file | `Io` | `read {path}: {e}` |
| `opencode.json` unreadable | `Io` | `read {path}: {e}` |
| `opencode.json` unparseable | `Io` | `parse {path}: {e}` |
| `opencode.json` not an object / `plugin` not an array | `InvalidArgument` | `{path} is not a JSON object; refusing to modify it` / `"plugin" in {path} is not an array; refusing to modify it` |
| Write failure | `Io` | `write {path}: {e}` (`atomic_write` already formats this way) |
| Backup failure before overwrite | `Io` | `read existing file {path} before backup: {e}` (reuses `backup_if_differs`) |
| Offline Mode blocks refresh | `ParanoidModeBlocked` | `feature: "rules_refresh"` (reuses `state.require_network`) |
| Second refresh while one runs | `InvalidArgument` | `rules refresh already in progress` (mirrors `corpus_refresh`) |

UI contract for errors: show `errorText(e)` (`types.ts:329`) verbatim inside the
`rules.error.*` heading. Never re-word a validation message (spec §8), never
truncate it, and never put a schema message behind a disclosure.

What is **not** an error:

- Missing rules corpus on first launch → `RuleStatus.emptyReason = "notSynced"`.
  Seeding runs before the read, so this means seeding genuinely produced nothing.
- Zero `.md` files under an existing `rules/` → `emptyReason = "noRules"`.
- A rule with `validation.valid == false` → **listed**, not an error (TASK 11).
  Only install/preview refuse it.

---

## 11. Conflicts to resolve before implementation

These are genuine contradictions between `rules-integration-spec.md`,
`rules-integration-tasks.md`, and the rulings. Each needs an orchestrator
decision; none can be settled by a dev in a PR.

**C1 — TASK 8 forbids what A6 requires.** `tasks.md:437-442` says "plugin
registration is out of M1. Do not write `opencode.json`", while A6
(`tasks.md:1345,1355-1377`) overrules it. **A6 wins**; TASK 8's paragraph must be
read as deleted. Consequence: TASK 16 gains a step and `rules/opencode.rs` exists.

**C2 — `dests` vs the plugin entry (needs a ruling).** TASK 16 says `dests` = "every
path written" and TASK 17 says uninstall deletes "every path in the row's
`dests`", but A6 says uninstall must **never** remove the plugin entry. If
`opencode.json` goes into `dests`, `rules_uninstall` deletes the user's OpenCode
config (or removes the whole file) — a data-loss bug that also breaks every other
installed rule. **Proposed resolution** (implement this unless overruled):
`dests` contains only the rendered rule file(s); `opencode.json` is a
side-effect write, recorded nowhere. Write that in a code comment at the ledger
construction site.

**C3 — `rules_install` return type.** TASK 16 says `Result<InstallRecord>`, TASK 18
lists `RuleInstall` as a new `types.ts` type. **Resolved here, no ruling needed**:
the Rust wire type stays `InstallRecord` (one struct, one serializer, no new Rust
type); TypeScript models the response as `RuleInstall = InstallRecord & { domain:
"rule" }`. Registering a second Rust DTO would duplicate 14 fields for a
discriminator.

**C4 — the ledger leaks into three agent surfaces that no task covers.** TASK 13
stores the **harness id** in `InstallRecord.tool`, and TASK 13/25 only filter
`installs_reconcile`, `installs_for_agent`. But three more readers walk the whole
ledger with no domain filter:

| Site | Symptom |
| --- | --- |
| `install/mod.rs:1118-1152` `tools_list` | `installed_count` for `antigravity` and `opencode` **double-counts**, because the rule harness ids `antigravity` and `opencode` are byte-equal to those agent tool ids. `pi-agent`/`oh-my-pi` are safe only by the kebab-vs-camelCase accident. |
| `install/mod.rs:1234-1256` `projects_list` | a rule row with `project_path` inflates that project's `installed_count`, i.e. a rule install shows up as an agent install in the Projects view — the exact thing spec §10.8 forbids. |
| `install/mod.rs:1281-1301` `loadout_export` | rule rows are written into the Agentfile; `loadout_import` then calls `do_install` with a harness id as a tool, fails, and silently skips. A user's Agentfile silently carries entries that never restore. |

All three need `.filter(|r| r.domain == InstallDomain::Agent)` and a TASK 25
assertion. This is an amendment to TASK 13, not a new task.

**C5 — the corpus has 8 rules, `integrations/` has 7.** `rules/base/` contains
`mcp-gateway-protocol.md`, which has **no compiled artifact** in any
`integrations/<harness>/` (all four `_manifest.json` list 7 files). TASK 10 says
"all 7 rules" and "28 comparisons", which is correct for the fixtures — but the
app's own seeded corpus will list 8. Two consequences: (a) `mcp-gateway-protocol`
has **zero** byte-parity coverage and must be added to the renderer unit tests;
(b) the A14 `#[ignore]`d `rules_parity_against_checkout` test **fails today** if it
treats a missing upstream artifact as a divergence — it must report "source rule
has no upstream artifact" as a distinct, non-fatal finding (or upstream must
regenerate `integrations/` first). Ask for a decision; do not silently skip.

**C6 — TASK 11's `RuleEntry` cannot support `rules_get`.** TASK 11 lists the index
fields and omits any source path, but slug ≠ filename (A3), so
`rules/base/commit-convention.md` (`name: commit-conventions`) is unreachable from
its slug. `relative_path` is added to `RuleEntry`/`Rule` above. Fold it into
TASK 3/TASK 11 acceptance criteria.

**C7 — TASK 3 says `#[serde(default)]` for optional fields, which loses
`alwaysApply: false`.** TASK 7 is explicitly keyed on *presence*
(`tasks.md:400-402`: "`alwaysApply: false` MUST be emitted"). `always_apply` must
be `Option<bool>`, not `bool` with a default. Encoding it as `bool` makes TASK 7
unimplementable.

**C8 — `rules_refresh` is a third command A5 did not approve.** A5 approved
`rules_status` and `rules_installs`; TASK 12 also adds `rules_refresh`
(`tasks.md:649-667`), which the spec's §3.5 list does not contain. It is
required (the `notSynced` empty state's CTA is useless without it, and spec §3.1
mandates the tarball refresh), so ask A5 to be extended rather than dropping it.

**C9 — A7's fallback branch is unreachable for the shipped contract.** All four
harness ids resolve through `kebab`, so "no registry match → OS home" never
executes in production. It must be covered by a synthetic unit test
(`harness_base` with `"not-a-harness"`) or it is untested dead code (ENG-07).

**C10 — A7 contradicts ambiguity #7, and A7 wins.** Ambiguity 7
(`tasks.md:1435-1440`) says harnesses deliberately do **not** map onto
`settings.tool_paths`; A7 (`tasks.md:1379-1391`) says they do. Implement A7. Note
in the PR description that ambiguity #7 is stale, so the next reader does not
"fix" the code back.

---

## 12. Data flow

```
 deuriib/agency-rules @ main
 ─────────────────────────────────────────────────────────────────────────────
   GitHub codeload tarball  ──►  rules_refresh (require_network, single-flight,
                                  strip top-level dir, preserve rules/** verbatim,
                                  atomic swap)                    TASK 12
                                          │
   bundled resources/rules-baseline/**  ──┤  first-run seed (is_empty_dir) TASK 12
                                          ▼
 ┌──────────────────────────────────────────────────────────────────────────┐
 │ <app_data>/rules-corpus/rules/<category>/<file>.md                       │
 └──────────────────────────────────────────────────────────────────────────┘
                                          │  build_rules_index            TASK 11
                                          │    parse.rs  → Rule
                                          │    validate.rs → RuleValidation
                                          │    sha256 (source_hash, body_hash)
                                          ▼
 ┌──────────────────────────────────────────────────────────────────────────┐
 │ <app_data>/state/rules-index.json   { count, rules: { slug: RuleEntry } } │
 │ <app_data>/state/rules-meta.json    { version, fetched_at, count }        │
 └──────────────────────────────────────────────────────────────────────────┘
                                          │
        ┌─────────────────────────────────┴──────────────────────────────┐
        │                                                                │
        ▼                                                                ▼
 rules_list / rules_get / rules_preview                        rules_status
        │                                                                │
        │  commands.rs orchestrates:                                     │
        │    render.rs  → render_rule_files ⊕ rule_dests                 │
        │    opencode.rs → plan_merge (pure)                             │
        ▼                                                                ▼
 ──────────────────── Tauri IPC · camelCase JSON · AppError{code,…} ─────────
        │
        ▼
 src/lib/stores/rules.svelte.ts   RulesStore ($state)
   list, loaded, loading, error, status, installations,
   previewCache (in-memory, key = `${slug}::${projectPath ?? ""}`)
        │
        │  $derived: query ∧ harnessFilter ∧ categoryFilter
        ▼
 src/lib/components/Rules.svelte ──selection──► src/lib/components/RuleDetail.svelte
   (header, search, filters, list pane)              ├─► RulePreview.svelte
                                                     └─► RuleInstallControls.svelte
                                                              │
                                                              │ invoke(rules_install)
                                                              ▼
 ┌──────────────────────────────────────────────────────────────────────────┐
 │ filesystem:                                                              │
 │   <base>/…rules/{{slug}}.md                (the rule)                    │
 │   <root>/opencode.json                     (opencode only, A6)           │
 ├──────────────────────────────────────────────────────────────────────────┤
 │ <app_data>/state/installs.json   InstallRecord { …, domain: "rule" }     │
 └──────────────────────────────────────────────────────────────────────────┘
                                                              │
                    rules_installs (re-read after every mutation — the ledger
                    is the source of truth, spec §10.6) ◄──────┘
```

Reading order for implementers: §2 → §3 → §5 → §6 (Rust), then §4 → §10 (commands),
then §8 → §9 (the two A6/A7 additions). §11 before any of it.
