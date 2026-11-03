# Rules Integration — Development Tasks (Issue #29, Milestone M1)

**Source of truth**: `docs/rules-integration-spec.md` (approved 2026-11-03).
**Issue**: [#29](https://github.com/deuriib/agency-agents-app/issues/29)
**Branch**: `feat/29-rules-integration`
**Scope**: M1 only — Rules catalog, compiled preview, install, ledger tracking.

Every requirement below is quoted from the spec. Where this document says
"spec §N", it means a section of `docs/rules-integration-spec.md`. If a task and
the spec disagree, the spec wins — stop and escalate.

---

## How to read a task

Each task is sized for **one dev–QA cycle** and must be independently
implementable and independently testable (target: ≤ ~200 lines of net change
excluding fixtures and tests). If a task grows past that, split it and say so in
the PR description.

Naming conventions used throughout (the spec anchors the *shape*, not these
exact names):

| Kind | Path |
| --- | --- |
| Rule DTOs (wire) | `src-tauri/src/types.rs` |
| Rules domain module | `src-tauri/src/rules/` |
| Bundled rule-harness contract | `src-tauri/data/rules-tools.json` |
| Bundled implemented rule formats | `src-tauri/data/rule-formats.json` |
| Bundled neutral rule schema | `src-tauri/data/rules-schema.json` |
| Rule harness TS mirror | `src/lib/data/ruleRegistry.ts` |
| Rule store | `src/lib/stores/rules.svelte.ts` |
| Rule workspace view | `src/lib/components/Rules.svelte` |
| Golden fixtures | `src-tauri/tests/fixtures/rules/<harness>/…` |

---

## Out of scope — binding

Spec §3, "Explicitly out of scope (M2+)", reproduced verbatim. **No task below
may implement any of these.** If an implementation "needs" one of them, the
design is wrong.

> - Drift detection and the five-state reconciliation for rules.
> - Assigning rules to Projects; the unified Projects view.
> - Dashboard health metrics for rules.
> - User-local custom rules directory.
> - Auto-update of installed rules.
> - Teams/loadouts that include rules.
> - Aggregated roster installs (the `_manifest.json` bundles are noted but not used).

Concretely, that means: no `rules_reconcile`, no `state` field on rule rows, no
`Rules` filter inside `Projects.svelte`, no rule counters on
`AgencyDashboard.svelte`, no foreign-rule adoption sweep, no `rules/` scan
outside the managed corpus root, and no use of `integrations/<harness>/_manifest.json`.

---

## Development Tasks

### [ ] TASK 1 — Bundle the rule-harness contract as declarative data (Rust reader)

**Dependencies**: none.

**Goal**: Make the four rule harnesses, their formats, and their destinations
data — read by Rust from an embedded JSON file, exactly like
`data/render-formats.json` is read by `registry.rs`. No destination string may
appear in Rust source.

**Files to touch**:
- `src-tauri/data/rules-tools.json` (new)
- `src-tauri/data/rule-formats.json` (new)
- `src-tauri/src/rules/registry.rs` (new)
- `src-tauri/src/rules/mod.rs` (new — `pub mod registry;` for now)
- `src-tauri/src/lib.rs` (`mod rules;`)
- `.vscode/settings.json` (optional `$schema` mapping, matching the existing
  `src-tauri/data/_schema.json` arrangement from `decisions.md` 2026-11-01)

**Exact requirements** (spec):

> Source of truth for the table above: `agency-rules/tools.json`
> (`harnesses.*.dest`). The app must read an equivalent declarative contract —
> it must **not** hardcode destinations in render code (see `registry.rs:12-14`:
> "never restate this list").

> `RuleHarness` — one of `antigravity`, `pi-agent`, `oh-my-pi`, `opencode`.

> Rule conformance is derived, never stored: a harness is installable iff a
> native renderer exists for its format.

> | Anchor | Role | How rules extend it |
> | `src-tauri/src/registry.rs:12-14` | Embedded `tools.json` + `render-formats.json`, single source of truth | Add a parallel declarative contract for rule harnesses and a matched pair of Rust + TS readers, exactly like `implemented_formats()`. |

**Acceptance criteria**:
- `rules-tools.json` carries a `harnesses` object with exactly four keys
  (`antigravity`, `pi-agent`, `oh-my-pi`, `opencode`); each entry's `id`,
  `label`, `format`, `installKind`, `integrationPath`, `dest.project`,
  `dest.user` is **byte-identical** to the corresponding entry in
  `/home/deuriib/projects/agency-rules/tools.json`. A verbatim copy of the
  `harnesses` object is the required shape; do not rename keys or "improve"
  values.
- `rule-formats.json` mirrors `render-formats.json`'s shape
  (`{"_note": "...", "formats": [...]}`) and lists exactly the four formats:
  `md-yaml-trigger`, `md-rules-dir`, `md-or-mdc`, `md-plugin`.
- `rules::registry::all()` returns harnesses in a deterministic order (sort by
  `label`, then `id` — the spec does not mandate an order, determinism is
  required for snapshot tests only).
- `rules::registry::get(id)` returns `Option<&'static RuleHarnessMeta>`.
- `rules::registry::installable()` yields only harnesses whose `format` is in
  `rule-formats.json`. Adding a harness with an unimplemented format must not
  make it installable.
- `rules::registry::implemented_formats()` reads `rule-formats.json` via
  `include_str!` — no format literal anywhere in Rust source.
- `dest` supports both scopes: `supports_user()` is `dest.user.is_some()`,
  `supports_project()` is `dest.project.is_some()` (derived, not stored).

**QA gate**: `cargo test --lib rules::registry` proves (a) four harnesses load,
(b) every harness's format resolves, (c) a harness added to the JSON with an
unknown format is *not* installable (inject via a test-only parse of a literal
JSON string, not by editing the bundled file), (d) `include_str!` is the only
source of the format list (grep-style assertion in the test, mirroring
`registry.rs`'s `render_formats_are_single_sourced_and_resolvable`).

---

### [ ] TASK 2 — Two sides cannot drift: TS mirror + contract guard

**Dependencies**: TASK 1.

**Goal**: The frontend reads the **same** two JSON files as Rust and derives the
same installable set, so the UI can never offer a harness the backend cannot
write. Guard the wiring with a test that fails if either side restates the list.

**Files to touch**:
- `src/lib/data/ruleRegistry.ts` (new)
- `scripts/check-rule-registry.mjs` (new)
- `scripts/check-rule-registry.test.mjs` (new)
- `src-tauri/src/rules/registry.rs` (test only, if the guard needs a hook)

**Exact requirements** (spec):

> Backend and frontend must read the **same** declarative JSON for rule
> harnesses, formats, and destinations (the two-sides-cannot-drift rule from
> `registry.rs:12-14` and issue #49).

> | `src/lib/data/toolRegistry.ts:1-30` | TS mirror of `tools.json` | Rules need the same TS/Rust lockstep so the UI cannot offer an uninstallable target. |

**Acceptance criteria**:
- `ruleRegistry.ts` imports
  `"../../../src-tauri/data/rules-tools.json"` and
  `"../../../src-tauri/data/rule-formats.json"` — the same two files Rust
  embeds. No harness id, format string, or destination literal appears in the
  file.
- `RULE_HARNESSES: RuleHarnessMeta[]`, `ruleHarness(id)`,
  `isHarnessInstallable(h)`, `installableHarnesses()`, `harnessLabel(id)` are
  exported, mirroring `toolRegistry.ts`'s shape and naming style.
- Types are strict TypeScript: no `any`, no `@ts-ignore` (ENG-01, ENG-04).
  Untrusted JSON is narrowed through a typed shape, not cast through
  `as unknown as` with no validation (a single narrow cast at the module
  boundary is acceptable, mirroring `toolRegistry.ts`).
- `scripts/check-rule-registry.mjs` exports:
  - `singleSourceWiring(root)` → `{ rust: boolean, frontend: boolean }`, true
    only when `rules/registry.rs` contains both `include_str!` paths and
    `ruleRegistry.ts` contains both JSON import paths.
  - `installableHarnesses(contract, formats)` → the derived set.
  - `formatCoverage(contract, formats)` → formats declared by a harness that
    are not implemented, and implemented formats no harness declares.
- `scripts/check-rule-registry.test.mjs` runs under `npm run test:scripts`
  (`node --test scripts/*.test.mjs`) and asserts: wiring both true; a
  reintroduced literal list fails; the Rust and TS derived sets are identical
  (parse the four harness ids from both files, compare as sorted arrays);
  format coverage has zero orphans on both sides.

**QA gate**: `npm run test:scripts` green, and a reviewer can point at the
failing assertion by temporarily hardcoding a harness in `ruleRegistry.ts` (the
test must fail, then be reverted). `mise run check` green.

---

### [ ] TASK 3 — Parse the neutral rule format into a typed `Rule`

**Dependencies**: TASK 1 (shape reference only).

**Goal**: Split a `rules/<category>/<slug>.md` file into typed frontmatter +
body, with the same three-hash split agents use, so M2 can classify updates
cosmetic vs substantive without a second parse.

**Files to touch**:
- `src-tauri/src/types.rs` (`Rule`, `RuleHarness` alias, `RuleEntry`)
- `src-tauri/src/rules/parse.rs` (new)
- `src-tauri/src/rules/mod.rs`

**Exact requirements** (spec):

> `Rule` — the neutral rule: `slug`, `name`, `description`, `category`, `globs`,
> `alwaysApply`, `trigger`, `match`, `tags`, `tools`, plus `source_hash` and
> `body_hash` (body split from frontmatter, as agents do, so M2 can classify
> updates cosmetic vs substantive).

> Neutral format is the only authored format. Each rule exists once under
> `rules/<category>/<slug>.md`; the four harness variants are compiled, never
> authored.

**Acceptance criteria**:
- `Rule.globs` accepts **both** authored shapes (`string` and `string[]`) per
  `rules/_schema.json`, normalizing to a single wire shape without losing which
  form was authored if the renderers need it (antigravity joins with commas
  either way; oh-my-pi/opencode preserve the shape they received — see TASK 7
  and TASK 8).
- Optional fields deserialize with `#[serde(default)]`; a rule with only
  `name` + `description` parses.
- **`slug` is the frontmatter `name`, never the filename.** This is proven by
  the corpus, not assumed: `rules/base/commit-convention.md` declares
  `name: commit-conventions`, and the compiled artifact in every
  `integrations/<harness>/` is `commit-conventions.md` / `commit-conventions/RULES.md`.
  A test must assert crossing filename ≠ slug.
- `source_hash` = SHA-256 of the whole file bytes; `body_hash` = SHA-256 of the
  body **after** the same normalization the renderers apply (`trim()`), so the
  hash matches the bytes that will be written. Reuse `render::sha256_hex`.
- `category` is the parent directory name.
- Deterministic: parsing the same bytes twice yields identical hashes.
- Files whose name starts with `_` (notably `_schema.json`) are skipped by the
  directory scanner and never surface as rules.

**QA gate**: `cargo test --lib rules::parse` covers: minimal rule
(name+description), `globs` as string, `globs` as array, `alwaysApply: false`
explicit, `trigger` with a trailing `#` comment (the corpus has one in
`rules/stacks/typescript.md` — the comment must be stripped, the value must
still be `glob`), the filename≠slug case, and `_`-prefix skipping.

---

### [ ] TASK 4 — Fail closed: validate frontmatter against the shipped schema

**Dependencies**: TASK 3.

**Goal**: A structurally invalid rule is rejected with a specific, verbatim
error and **no file is written**.

**Files to touch**:
- `src-tauri/data/rules-schema.json` (new — verbatim copy of
  `agency-rules/rules/_schema.json`)
- `src-tauri/src/rules/validate.rs` (new)
- `src-tauri/src/rules/mod.rs`
- `src-tauri/src/types.rs` (`RuleValidation`)

**Exact requirements** (spec):

> **Required** — frontmatter is validated against the schema before any compile
> or write.

> Rule parsing + validation: neutral frontmatter parsed into a typed `Rule`;
> validated against the shipped JSON Schema before compile/install.

> Invalid frontmatter → validation error surfaced verbatim, **no file written**.

> Path traversal: `slug` comes from frontmatter and is schema-constrained to
> `^[a-z][a-z0-9-]*$`; re-validate in Rust before any path join (DENY-03).

**Acceptance criteria**:
- `rules-schema.json` is a byte-identical copy of the upstream schema (draft-07,
  `additionalProperties: false`, `required: ["name","description"]`). The sync
  from upstream must not drop keys; do not add `$schema` to the source.
- Validation uses the `jsonschema` crate already present as a dev-dependency —
  **promote it to a runtime dependency** with `default-features = false`
  (matching `src-tauri/tests/tools_schema.rs`), rather than adding a second
  schema library.
- `RuleValidation { valid: bool, errors: Vec<String> }` is serialized on the
  wire and attached to every rule the UI sees (spec §7: "validation result").
- Error strings are the schema validator's own messages, unprefixed and
  untruncated ("surfaced verbatim"). No rewording, no summarizing.
- An unknown key (e.g. `alwayApply`) fails: `additionalProperties: false`.
- A `name` violating `^[a-z][a-z0-9-]*$` or exceeding 64 chars fails.
- A `description` of 0 or >1024 chars fails.
- Negative-case tests are mandatory so the schema cannot rot into a no-op
  (same policy as `src-tauri/tests/tools_schema.rs`, `decisions.md` 2026-11-01).
- Validating is pure — no filesystem access in `validate.rs`.

**QA gate**: `cargo test --lib rules::validate` asserts each negative case above
and that a corpus-real rule validates clean. A test must prove no write path is
reachable from a failed validation (the install path simply does not exist yet;
TASK 16 proves the integration).

---

### [ ] TASK 5 — Antigravity rule renderer (`md-yaml-trigger`)

**Dependencies**: TASK 1, TASK 3.

**Goal**: The first native rule renderer, plus the shared `wrap_rule` primitive
the other three reuse. Byte-identical to
`agency-rules/scripts/lib/renderers/antigravity.mjs`.

**Files to touch**:
- `src-tauri/src/rules/render.rs` (new — `wrap_rule` + `render_antigravity`)
- `src-tauri/src/rules/mod.rs`

**Exact requirements** (spec):

> **Antigravity** — requires `trigger`. Precedence: explicit `trigger` →
> `alwaysApply: true` ⇒ `always_on` → `globs` present ⇒ `glob` → only
> `description` ⇒ `model_decision` → otherwise `manual`. When `trigger: glob`,
> `globs` is joined with **commas** into a single string.

> Determinism is REQUIRED: identical input must produce identical bytes so
> `rendered_hash` is stable (same contract as agent renders).

Reference renderer (`scripts/lib/renderers/antigravity.mjs` + `_util.mjs`):

```js
const fm = { trigger };
if (trigger === 'glob' && d.globs) {
    fm.globs = Array.isArray(d.globs) ? d.globs.join(',') : d.globs;
}
if (d.description) fm.description = d.description;
return wrap(fm, rule.body);

// wrap:
const fmText = yamlStringify(data).trimEnd();
const content = (body ?? '').trim();
return `---\n${fmText}\n---\n\n${content}\n`;
```

**Acceptance criteria**:
- `wrap_rule(pairs: &[(&str, YamlValue)], body: &str) -> String` emits exactly
  `---\n<yaml>\n---\n\n<trimmed body>\n`. Note the **blank line** after the
  closing `---` — this differs from the agent renderers in `render/mod.rs`,
  which emit `---\n…\n---\n{body}\n`. Do not reuse the agent template.
- Field order is the caller's order: `trigger`, then `globs` (only when
  `trigger == "glob"` and globs are present), then `description` (only when
  non-empty).
- **Emit YAML by hand, not with `serde_yaml`.** The reference emits
  `globs:\n  - "**/*.ts"\n  - "**/*.tsx"` (2-space indented block sequence,
  double-quoted scalars). `serde_yaml` emits `globs:\n- "**/*.ts"` with no
  indent, which would fail conformance on every globbed rule. A purpose-built
  emitter over the fixed field set is the required approach; record the reason
  in a comment (ENG-13: no abstraction without a pain point, but also no
  library that silently changes bytes).
- Scalar quoting must match for at least: plain safes (`glob`, `always_on`,
  `model_decision`, `manual`), and strings that need quoting — leading `*`,
  leading/trailing space, embedded `: `, leading `#`, and anything that would
  parse as a YAML boolean/number. `"**/*.ts,**/*.tsx"` must be quoted.
- Precedence is implemented in the documented order and covered by a test per
  branch (five branches + the explicit-trigger override).
- `globs` joined with `,` and **no space**, whether authored as string or array.
- Pure function: no I/O, no clock, no map iteration into output.

**QA gate**: `cargo test --lib rules::render::antigravity` covers all five
precedence branches, both `globs` authored shapes, explicit `trigger`
overriding a `globs`-present rule, and determinism (render twice, assert equal).
TASK 10 then proves byte parity against the real artifact.

---

### [ ] TASK 6 — pi-agent rule renderer (`md-rules-dir`)

**Dependencies**: TASK 5 (`wrap_rule`).

**Files to touch**:
- `src-tauri/src/rules/render.rs`

**Exact requirements** (spec):

> **pi-agent** — emits `name`, `description`, `tools`; drops `globs`,
> `alwaysApply`, `trigger`, `match`, `tags`. One directory per rule with
> `RULES.md` inside.

**Acceptance criteria**:
- Field order: `name`, `description`, then `tools` **only when it is a
  non-empty array** (an empty array emits nothing — the reference guards with
  `Array.isArray(d.tools) && d.tools.length > 0`).
- `globs`, `alwaysApply`, `trigger`, `match`, `tags` are never emitted, even
  when present on the rule. A test asserts their absence as substrings.
- `tools` is emitted as a 2-space block sequence of double-quoted scalars.
- The directory-ness is **not** this renderer's business: it returns one
  `RenderedFile` and the destination template (`{{slug}}/RULES.md`) supplies the
  nesting. Do not emit a path here.

**QA gate**: `cargo test --lib rules::render::pi_agent` covers: rule with
`tools`, rule with empty `tools` (dropped), rule with `globs`+`alwaysApply`+`tags`
present (all dropped), and determinism.

---

### [ ] TASK 7 — oh-my-pi rule renderer (`md-or-mdc`)

**Dependencies**: TASK 5 (`wrap_rule`).

**Files to touch**:
- `src-tauri/src/rules/render.rs`

**Exact requirements** (spec):

> **oh-my-pi** — emits `name`, `description`, `globs`, `alwaysApply`; drops the
> rest.

**Acceptance criteria**:
- Field order: `name`, `description`, then `globs` when `globs` is present and
  not `null`, then `alwaysApply` when present — both keyed on **presence**, not
  truthiness. `alwaysApply: false` MUST be emitted (the reference checks
  `!== undefined`). A test must assert the emitted `alwaysApply: false` line —
  this is the single most likely conformance bug.
- `globs` preserves the authored shape: an array stays a block sequence, a
  string stays a quoted scalar. Both shapes appear in the corpus
  (`rules/stacks/typescript.md` is an array, `rules/stacks/pyhton.md`'s
  `globs: ["**/*.py"]` is a flow array that must render as a block sequence —
  the reference normalizes both to a block sequence via the YAML serializer, so
  a single-element array still emits `globs:\n  - "**/*.py"`).
- `trigger`, `match`, `tags`, `tools` are never emitted.
- Extension stays `.md` (the harness accepts `.md` or `.mdc`; `.md` is chosen).

**QA gate**: `cargo test --lib rules::render::oh_my_pi` covers `alwaysApply:
false` (emitted), `alwaysApply: true` (emitted), `alwaysApply` absent (omitted),
array globs, single-element array globs, string globs, and dropped-field
assertions.

---

### [ ] TASK 8 — OpenCode rule renderer (`md-plugin`)

**Dependencies**: TASK 5 (`wrap_rule`).

**Files to touch**:
- `src-tauri/src/rules/render.rs`

**Exact requirements** (spec):

> **OpenCode** — emits `description`, `globs`, `match`; drops `name`,
> `alwaysApply`, `trigger`, `tags`, `tools`.

**Acceptance criteria**:
- Field order: `description`, then `globs` when present, then `match` when
  present. `name` is **not** emitted, even though the rule has one — a test
  asserts the absence of `name:` in the output frontmatter.
- `globs` follows the same presence + shape rules as TASK 7.
- `match` emits only `any` or `all` (schema-constrained); anything else cannot
  reach here because TASK 4 rejects it first — but the renderer must not panic
  if it somehow does.
- The `plugin` block in `rules-tools.json` (`opencode-rules`) is **not** this
  task's concern. Spec §3 excludes nothing about plugin registration, but §3.5
  lists only list/get/preview/install/uninstall — so plugin registration is
  out of M1. Do not write `opencode.json`. (See Ambiguities, A6.)

**QA gate**: `cargo test --lib rules::render::opencode` covers `match` present,
`match` absent, array globs, string globs, and asserts `name:`,
`alwaysApply:`, `trigger:`, `tags:`, `tools:` are absent from the output.

---

### [ ] TASK 9 — Rule render entry point, destinations, and slug re-validation

**Dependencies**: TASK 1, TASK 5, TASK 6, TASK 7, TASK 8.

**Goal**: One dispatch point mirroring `render::render_files`, plus destination
resolution from the declarative contract — with the slug re-validated before any
path join.

**Files to touch**:
- `src-tauri/src/rules/render.rs`
- `src-tauri/src/rules/mod.rs`

**Exact requirements** (spec):

> | Harness | Format | Project destination | User destination |
> | Google Antigravity | `md-yaml-trigger` | `.agents/rules/{{slug}}.md` | `~/.gemini/config/rules/{{slug}}.md` |
> | pi-agent | `md-rules-dir` | `.pi/rules/{{slug}}/RULES.md` | `~/.pi/agent/rules/{{slug}}/RULES.md` |
> | oh-my-pi (omp) | `md-or-mdc` | `.omp/rules/{{slug}}.md` | `~/.omp/agent/rules/{{slug}}.md` |
> | OpenCode | `md-plugin` | `.opencode/rules/{{slug}}.md` | `~/.config/opencode/rules/{{slug}}.md` |

> Unknown harness → never offered in the UI.

> Path traversal: `slug` comes from frontmatter and is schema-constrained to
> `^[a-z][a-z0-9-]*$`; re-validate in Rust before any path join (DENY-03).

> Reuse `RenderedFile` and `sha256_hex`; add rule renderers beside the agent ones.

**Acceptance criteria**:
- `render_rule_files(rule, harness) -> Result<Vec<RenderedFile>, AppError>`
  dispatches on the harness's `format` read from the registry. Returns exactly
  one file for all four harnesses; `RenderedFile.relative` stays `""` because the
  destination template already carries the nesting (`{slug}/RULES.md`).
- `rule_dests(harness, slug, home, project_root) -> Result<Vec<PathBuf>, AppError>`
  substitutes `{slug}` in the template the contract declares, expanding a
  leading `~/` against `home`. It must not special-case any harness id.
- **`{slug}` (single braces)** is the placeholder, shared with the agent
  contract (issue #68 supersedes the original double-brace form). A template
  left with the legacy `{{slug}}` resolves to `{…}` and is rejected rather than
  written: a test asserts the substitution happened and no brace survives in any
  resolved path.
- Slug is re-validated against `^[a-z][a-z0-9-]*$` **inside** the dest resolver,
  before `root.join(...)`, and again before any write in TASK 16. A slug like
  `../../etc/passwd` or `a/b` returns an error; nothing is joined.
- Rejecting an unknown harness returns a typed `AppError`, never a panic and
  never a default path.
- Project scope with no project root returns the same style of error the agent
  path returns for a project-scoped tool without a root
  (`"… a project path is required"`), not a silent `~` write.

**QA gate**: `cargo test --lib rules::render` covers: all four harnesses resolve
one dest under user scope; all four resolve one dest under a temp project root;
pi-agent's dest nests one directory; `../../etc/passwd`, `A-B`, `a/b`, `""` are
rejected with zero filesystem contact (assert with a `tempfile::tempdir()` that
stays empty); unknown harness rejected; no `{{` in output.

---

### [ ] TASK 10 — Golden-fixture conformance against the real `integrations/`

**Dependencies**: TASK 5, TASK 6, TASK 7, TASK 8, TASK 9.

**Goal**: Prove byte-identical output — not "close", not "equivalent". This is
the load-bearing contract: a single byte of drift makes `rendered_hash`
meaningless and M2 reconciliation (drift/outdated) impossible to build.

**Files to touch**:
- `src-tauri/tests/fixtures/rules/antigravity/*.md` (7 files, copied)
- `src-tauri/tests/fixtures/rules/pi-agent/*/RULES.md` (7 files, copied)
- `src-tauri/tests/fixtures/rules/oh-my-pi/*.md` (7 files, copied)
- `src-tauri/tests/fixtures/rules/opencode/*.md` (7 files, copied)
- `src-tauri/tests/fixtures/rules/sources/*.md` (7 source rules, copied)
- `src-tauri/tests/rules_conformance.rs` (new)

**Exact requirements** (spec):

> Each renderer must reproduce the reference output byte-for-byte, verified by
> golden fixtures taken from `agency-rules/integrations/<harness>/`.

> New Rust logic covered by unit tests; renderers covered by golden-fixture
> tests (TST-01, TST-03).

**Acceptance criteria**:
- Fixtures are copied **verbatim** from
  `/home/deuriib/projects/agency-rules/integrations/<harness>/` and
  `/home/deuriib/projects/agency-rules/rules/` (all 7 rules:
  `agentmemory`, `coding-standards`, `commit-conventions`, `nexus-strategy`,
  `python`, `subagent-delegation`, `typescript`). Do not hand-edit a fixture to
  make a test pass — a mismatch is a renderer bug.
- `rules_conformance.rs` walks the fixture tree, renders each source for each
  harness, and asserts byte equality including trailing newlines and the blank
  line after `---`.
- The test prints a `diff`-style first-divergence report (offset + expected vs
  actual bytes) so a failure is diagnosable without re-reading the renderer.
- A stale-fixture guard: an `#[ignore]`d test that, when
  `AGENCY_RULES_PARITY_ROOT` is set to a real `agency-rules` checkout, re-renders
  from that checkout's `rules/` and diffs against `integrations/`. Mirrors the
  existing `mise run parity` arrangement (`decisions.md` 2026-06-14,
  `systemPatterns.md` §3). Document it in `docs/BUILD.md` next to the existing
  parity note.
- The corpus has **no** rule exercising `tools` or `match`, so fixtures cannot
  cover them. Those two fields are covered by TASK 6/7/8 unit tests only —
  state that coverage gap in the test file's header comment so nobody assumes
  the fixtures are exhaustive.

**QA gate**: `cargo test --test rules_conformance` green: 7 sources × 4
harnesses = 28 byte-exact comparisons. Reviewer confirms the fixtures are
byte-identical to the sibling repo (`diff -r`), and confirms the test fails when
one byte is perturbed in a renderer.

---

### [ ] TASK 11 — Rules corpus: root resolution and `rules-index.json`

**Dependencies**: TASK 1, TASK 3, TASK 4.

**Goal**: A second managed corpus root for `agency-rules`, scanned into a
deterministic index. No network in this task.

**Files to touch**:
- `src-tauri/src/rules/corpus.rs` (new)
- `src-tauri/src/rules/mod.rs`
- `src-tauri/src/corpus/mod.rs` (only if a shared path helper must be made
  `pub(crate)` — prefer reuse over duplication)

**Exact requirements** (spec):

> Corpus: `deuriib/agency-rules` becomes a second managed corpus with its own
> baseline seed, tarball refresh, and `rules-index.json`.

> Rules catalog source — **Corpus-copy of `deuriib/agency-rules`**: Mirrors
> `systemPatterns.md §1` exactly: bundled baseline → extract to app-data →
> refresh from the GitHub tarball → rebuild an index. No runtime git dependency.

> Missing corpus → empty state that distinguishes "not synced" from "no rules
> shipped", mirroring the lesson recorded in `decisions.md` 2026-11-02.

**Acceptance criteria**:
- `rules_corpus_dir(app_data_dir)` = `<app_data>/rules-corpus`;
  `rules_index_path(app_data_dir)` = `<app_data>/state/rules-index.json`. It must
  **not** share `corpus/`, `corpus-index.json`, or `corpus-meta.json` with the
  agent corpus — the two repos refresh independently.
- `build_rules_index(root) -> Result<BTreeMap<String, RuleEntry>>` walks
  `rules/<category>/*.md`, skipping any path segment starting with `_` and any
  non-`.md` file (so `rules/_schema.json` never appears).
- Index rows are keyed by the **frontmatter `name`** (the slug), `BTreeMap` for
  stable key order, and carry `slug`, `name`, `description`, `category`,
  `source_hash`, `body_hash`, plus the parsed optional fields needed by the
  renderers. No timestamps in the index (determinism — same contract as
  `corpus-index.json`, `systemPatterns.md §2`).
- A duplicate slug across categories is reported as an error naming both paths,
  not silently deduped (upstream `validate.mjs` treats uniqueness as fatal).
- A structurally invalid rule is **retained** in the index with its
  `RuleValidation`, so the UI can list it and show why it cannot be installed —
  it is filtered out of *install*, not out of *list*. (Spec §10.4 requires a
  visible error, not an invisible rule.)
- Missing root, root-without-`rules/`, and unreadable-file are three distinct
  errors, mirroring `read_runbooks`' "`Ok(vec![])` only when the manifest is
  absent; every other failure carries the path and the reason".
- Determinism test: building the index twice over the same tempdir yields
  identical serialized bytes.

**QA gate**: `cargo test --lib rules::corpus` covers: empty tempdir (# distinct
from unreadable), a tempdir with `rules/base/ok.md` (indexed), `_schema.json`
present (skipped), duplicate slugs across two categories (error names both),
a rule failing validation (indexed + invalid), and byte-identical double build.

---

### [ ] TASK 12 — Rules corpus: bundled baseline seed + tarball refresh

**Dependencies**: TASK 11.

**Files to touch**:
- `src-tauri/resources/rules-baseline/**` (new — vendored seed)
- `src-tauri/tauri.conf.json` (`bundle.resources` gets
  `"resources/rules-baseline/**/*"`)
- `src-tauri/src/rules/corpus.rs`
- `src-tauri/src/rules/commands.rs` (new — `rules_refresh`)
- `src-tauri/src/lib.rs` (`rules::commands::rules_refresh` in `invoke_handler`)

**Exact requirements** (spec):

> **Seed**: a bundled baseline corpus ships inside the app bundle (resources) so
> first launch works offline. On first run it is extracted to `corpus/`.

> **Refresh**: fetch the GitHub tarball… extract, atomically swap `corpus/`,
> rebuild `corpus-index.json`. Record the resolved commit SHA / tag as
> `corpus_version`. **No runtime git dependency.**
> *(quoted from `systemPatterns.md` §1, which spec §1.1 adopts for rules)*

> Missing corpus → empty state that distinguishes "not synced" from "no rules
> shipped".

**Acceptance criteria**:
- Baseline = `agency-rules/rules/` copied to
  `src-tauri/resources/rules-baseline/rules/`, plus `_schema.json`, so a fresh
  offline install has real rules.
- Seed runs on first launch when `<app_data>/rules-corpus` is empty, reusing the
  existing atomic-swap / `is_empty_dir` approach from `corpus::resolve_active`
  rather than a new one.
- `rules_refresh(app, state)`:
  - calls `state.require_network("rules_refresh")` first (Offline Mode is a hard
    gate — same pattern as `corpus_refresh`),
  - single-flights (`try_lock`, fast-fail on a second click),
  - fetches `https://codeload.github.com/deuriib/agency-rules/tar.gz/refs/heads/main`
    through the existing capped/timeout download helper pattern,
  - strips the single top-level `<repo>-main/` dir, preserves `rules/**`
    verbatim (the `decisions.md` 2026-11-02 lesson: **no allow-list of
    directories**), and atomically swaps,
  - rebuilds `rules-index.json`, writes `rules-meta.json` with
    `version`/`fetched_at`/`count`,
  - is path-traversal safe (destinations rebuilt from validated components only,
    never from the raw archive path).
- Baseline and refresh both leave `rules/<category>/` layout intact so TASK 11's
  scanner needs no branch.
- `rules_status()` (small addition, same command file) reports
  `{ configured, count, version, empty_reason: "notSynced" | "noRules" | null }`
  so the UI can distinguish the two empty states. Without it the empty state
  cannot satisfy spec §8.

**QA gate**: `cargo test --lib rules::corpus` covers: seed into an empty root
(real files land, index rebuilt), seed is a no-op on a populated root, and a
synthetic tarball (the existing `tarball()` test helper pattern in
`corpus/mod.rs`) containing `rules/base/x.md` + a file outside `rules/`
extracts the first and ignores the second. A manual run of `rules_refresh`
against the live repo is confirmed in the PR description (not automatable in CI).

---

### [ ] TASK 13 — Ledger: add the domain discriminator without moving agent bytes

**Dependencies**: TASK 3.

**Goal**: Rules are recorded in the **existing** `installs.json`, distinguishable
from agent rows, reversible, invisible to agent reconciliation — and a v0.8.x
ledger still parses and re-serializes byte-identically.

**Files to touch**:
- `src-tauri/src/types.rs` (`InstallDomain` enum, `InstallRecord.domain`)
- `src-tauri/src/install/mod.rs` (`record_for`, `installs_reconcile`,
  `installs_for_agent`, ledger round-trip)

**Exact requirements** (spec):

> Extend the local ledger to include rules.

> Ledger rows gain a domain discriminator. Requirement: a ledger written by
> v0.8.x must still parse, and agent rows must be byte-identical on
> re-serialize. Rules must not appear in agent reconciliation output.

> | `src-tauri/src/types.rs:196` | `InstallRecord` | Add an optional discriminator so old ledgers keep parsing (`#[serde(default)]`). |

**Acceptance criteria**:
- `InstallDomain { Agent, Rule }`, `#[serde(rename_all = "camelCase")]`,
  `Default = Agent`, and the new `InstallRecord.domain` field is
  `#[serde(default, skip_serializing_if = "is_agent")]`.
  **`skip_serializing_if` is mandatory** — without it every agent row gains a
  `"domain": "agent"` key and the byte-identical requirement fails.
- Reusing `slug` for the rule slug and `tool` for the harness id is the
  intended extension (both are already `String`); no parallel field pairs.
- `installs_reconcile` filters `domain == Agent` before classifying. A rule row
  on disk must never produce a `Foreign` agent, an `Outdated` agent, or any
  `InstalledAgent`. Test with a ledger containing both kinds plus a real file at
  the rule's dest.
- `installs_for_agent(slug)` filters `domain == Agent`.
- A fixture ledger written by v0.8.x (captured from a real app-data
  `installs.json`, multi-file + `fileHashes` + `aggregateSlugs` present) is
  committed as `src-tauri/tests/fixtures/installs-v0.8.x.json`; the test parses
  it, re-serializes with `serde_json::to_vec_pretty`, and asserts **byte
  equality with the fixture**. This is the regression gate.
- `load_ledger`'s `dests` backfill still applies; rule rows record `dest` = the
  primary written path and `dests` = every path written.

**QA gate**: `cargo test --lib install` proves: (a) v0.8.x fixture round-trips
byte-identically, (b) a rule row serializes with `"domain": "rule"`, (c) an
agent row serializes with **no** `domain` key, (d) reconcile over a mixed ledger
returns only agent rows, (e) `installs_for_agent` over a mixed ledger returns
only that agent's rows.

---

### [ ] TASK 14 — Commands: `rules_list` and `rules_get`

**Dependencies**: TASK 3, TASK 4, TASK 11, TASK 12.

**Files to touch**:
- `src-tauri/src/rules/commands.rs`
- `src-tauri/src/rules/mod.rs`
- `src-tauri/src/lib.rs` (`invoke_handler`)
- `src/lib/types.ts` (mirror `Rule`, `RuleValidation`, `RulesStatus`)

**Exact requirements** (spec):

> Commands: a minimal Tauri command surface for list/get/preview/install/
> uninstall of rules.

> | `src-tauri/src/corpus/mod.rs:491,1471-1864` | `resolve_active`, `corpus_*`, `catalog_*`, `runbooks_list` | `runbooks_list` is the precedent for exposing a second content type from the catalog root. |

> Rule list: search plus filters by harness and by category.
> *(§7 — the filtering itself may be client-side; the payload must carry the fields it needs)*

**Acceptance criteria**:
- `rules_list(app, state, category: Option<String>) -> Result<Vec<RuleSummary>, AppError>`
  returns bodies **omitted** (like `corpus_list`) and includes `slug`, `name`,
  `description`, `category`, `globs`, `alwaysApply`, `trigger`, `match`, `tags`,
  `tools`, and `validation`, so the client can filter and badge without N+1
  calls.
- `rules_get(app, state, slug) -> Result<Rule, AppError>` returns the full rule
  including the neutral `body`, `source_hash`, `body_hash`, `category`, and
  `validation`. Unknown slug → `AppError::InvalidArgument` naming the slug
  (mirror `corpus_get`), never an empty `Rule`.
- Both are registered in `lib.rs`'s `invoke_handler` as
  `rules::commands::rules_list` / `rules::commands::rules_get`.
- Both read through TASK 11's corpus resolution (which seeds on first use), so a
  fresh install lists the bundled baseline rules with no network.
- An invalid rule appears in `rules_list` with `validation.valid == false` and
  its errors; it does not abort the whole list.
- Deterministic ordering: by `category`, then `name`.

**QA gate**: `cargo test` proves list shape (bodies empty), get shape (body
populated), unknown-slug error, invalid rule present-with-errors, and stable
ordering. `mise run check` proves the TS mirror type-checks against the Rust
serialization (`camelCase` on both sides).

---

### [ ] TASK 15 — Command: `rules_preview`

**Dependencies**: TASK 9, TASK 14.

**Files to touch**:
- `src-tauri/src/rules/commands.rs`
- `src-tauri/src/lib.rs`
- `src/lib/types.ts` (`RulePreview`)

**Exact requirements** (spec):

> Allow preview of how the compiled rule will look.

> Compiled preview: switch between the four harnesses and see the exact bytes
> that would be written, with the destination path shown.

> On-demand or pre-compiled generation.

**Acceptance criteria**:
- `rules_preview(app, state, slug, project_path: Option<String>) -> Result<RulePreview, AppError>`
  returns one entry per **installable** harness (unknown/unsupported harnesses
  are absent, per §8 "Unknown harness → never offered in the UI"), each with
  `harness`, `bytes`, `dest` (absolute), `rendered_hash`, and `installable`.
- On-demand only. Nothing is written to disk, nothing is cached to disk, and
  `dest` is computed but never created — a test asserts the tempdir is still
  empty after a preview.
- `project_path: Some(root)` resolves project-scope dests against `root`;
  `None` resolves user-scope dests. The response states which scope it
  previewed so the UI never shows a user path while the user has a project
  selected.
- An **invalid** rule returns an error carrying the validation messages and
  **no** bytes — preview must not show output for a rule that cannot be
  installed (§10.4: "no filesystem write" plus "visible, specific validation
  error").
- Bytes are exactly what TASK 16 would write: a test renders through the preview
  path, installs through TASK 16's path, and compares the file bytes and the
  `rendered_hash`.

**QA gate**: `cargo test` proves: four harness entries, zero writes to a tempdir,
user vs project dest difference, invalid rule → error with verbatim messages and
no bytes, and preview-bytes == installed-bytes for at least two harnesses.

---

### [ ] TASK 16 — Command: `rules_install`

**Dependencies**: TASK 4, TASK 9, TASK 13, TASK 14.

**Files to touch**:
- `src-tauri/src/rules/commands.rs`
- `src-tauri/src/install/mod.rs` (shared write/backup helper, if reusable
  without contortion)
- `src-tauri/src/lib.rs`

**Exact requirements** (spec):

> Installing a rule writes the correct bytes to the correct destination for the
> chosen harness and scope, and records the action in the ledger.

> Invalid frontmatter → validation error surfaced verbatim, **no file written**.

> Write failure → no partial file; report the path and the OS error.

> Project scope requires a project root the user picks — no silent writes to an
> arbitrary directory.

> Install controls: per-rule, per-harness, with scope selection where the
> harness supports it (user / project).

**Acceptance criteria**:
- `rules_install(app, state, slug, harness, project_path: Option<String>) -> Result<InstallRecord, AppError>`.
- Order is fixed and fail-closed: resolve rule → **validate** → check harness is
  installable → resolve dests → refuse project scope without `project_path` →
  write → ledger. A validation failure, an unknown harness, or a missing project
  root must each return before any `create_dir_all`/write.
- Re-validate the slug before the path join (TASK 9's check is repeated at the
  write boundary — DENY-03 wants the guard where the write happens).
- Writes are **atomic** (`util::fs::atomic_write`) so a failure mid-write leaves
  no partial file, and nested dirs (`…/{{slug}}/RULES.md`) are created before
  the write.
- An existing different file at `dest` is backed up before overwrite, reusing
  `install::backup_if_differs` (or an extracted equivalent) — byte-identical
  files are a no-op, matching the agent path's behaviour.
- Ledger row: `domain: Rule`, `slug` = rule slug, `tool` = harness id, `scope`
  from `render::scope_for(project_path)`, `project_path`, `dest`, `dests`,
  `source_hash` = rule `source_hash`, `body_hash` = rule `body_hash`,
  `rendered_hash` = hash of the exact bytes written, `installed_at`, and a
  `corpus_version` from `rules-meta.json`.
- Re-installing the same (slug, harness, project_path) **replaces** the row
  (retain-then-push, as `do_install` does) rather than appending a duplicate.
- The persisted dest is absolute and inside the resolved root. A test asserts no
  resolved path escapes `home`/`project_root` for a hostile slug.

**QA gate**: `cargo test` proves: correct bytes at the correct path for all four
harnesses (user scope in a tempdir home, project scope in a tempdir root);
invalid rule → zero files + verbatim errors; unknown harness → error; project
scope without a root → error and no write; a second install of the same key
leaves exactly one ledger row; a pre-existing divergent file is backed up;
atomicity (inject a write failure and assert the target does not exist).

---

### [ ] TASK 17 — Command: `rules_uninstall`

**Dependencies**: TASK 13, TASK 14.

**Files to touch**:
- `src-tauri/src/rules/commands.rs`
- `src-tauri/src/lib.rs`

**Exact requirements** (spec):

> Uninstalling removes the file(s) the app wrote and the ledger row.

> Uninstall must be available for anything the app installed.

**Acceptance criteria**:
- `rules_uninstall(app, state, slug, harness, project_path: Option<String>) -> Result<(), AppError>`
  deletes every path in the row's `dests` and then removes that ledger row. It
  never deletes a path that is not in `dests` (no wildcards, no directory walks).
- A missing file (already gone) is not an error; the row is still removed. A
  permission failure IS an error, reports the path and the OS error, and leaves
  the row in place (so the UI can retry rather than losing the record).
- Only `domain == Rule` rows are matched. Calling it for an agent slug cannot
  delete an agent install.
- The now-empty `{{slug}}/` directory pi-agent leaves behind is **not** removed
  (§10.7 says file(s), not directory). Record that choice in a code comment.
- `rules_installs(app)` (small companion, same file) returns the rule rows only,
  so the UI can render install state after a restart from the ledger — the
  ledger is the source of truth, not component state (§10.6).

**QA gate**: `cargo test` proves: file removed + row removed; missing file →
row removed, no error; a path not in `dests` untouched; agent rows unaffected;
`rules_installs` returns only rule rows; `rules_installs` reflects a hand-written
ledger (restart simulation) with no in-memory state.

---

### [ ] TASK 18 — Frontend store: `rules.svelte.ts`

**Dependencies**: TASK 2, TASK 14, TASK 15, TASK 16, TASK 17.

**Files to touch**:
- `src/lib/stores/rules.svelte.ts` (new)
- `src/lib/types.ts` (`Rule`, `RuleSummary`, `RulePreview`, `RuleInstall`)

**Exact requirements** (spec):

> | `src/lib/components/Runbooks.svelte` + `src/lib/stores/runbooks.svelte.ts` | Smallest existing example of a catalog-backed content section | Mirror its shape for the rules store and view. |

> Loading, empty, error, and partial states are all rendered (ENG-19).

> The install is visible in the app after a restart (ledger is the source of
> truth, not component state).

**Acceptance criteria**:
- Mirrors `RunbooksStore`'s contract: `list`, `loaded`, `loading`, `error`,
  `status`, `installations`, plus `load()`, `loadBody(slug)`, `preview(slug,
  projectPath)`, `install(slug, harness, projectPath)`, `uninstall(...)`.
- `loaded` is distinct from `list.length === 0` so "empty" ≠ "not fetched yet".
- A read/parse failure surfaces `error` (a message ready to display); it never
  degrades to the empty state — the `runbooks.svelte.ts` doc comment records why
  (`decisions.md` 2026-11-02 / issue #58).
- `status.emptyReason` distinguishes `"notSynced"` from `"noRules"` and the store
  exposes it verbatim for the UI to branch on.
- Installations are re-read from `rules_installs` after every install/uninstall
  (no local mutation of install state), so a restart shows the same truth.
- Per-rule previews are cached **in memory only**, keyed by `slug + projectPath`,
  and invalidated on install/uninstall.
- Strict TypeScript, no `any`, no `@ts-ignore` (ENG-01, ENG-04). `errorText(e)`
  from `$lib/types` is used for error normalization, as `runbooks.svelte.ts` does.
- Touch-toast/journal side effects stay in the component or reuse the existing
  `toast` store — do not import the agent `install.svelte.ts` journal (rules are
  not agents and must not pollute the agent Activity journal).

**QA gate**: `mise run check` green. Reviewer confirms every store field maps to
a real UI state branch in TASK 19–21 (no dead fields) and that the store holds no
derived install truth of its own.

---

### [ ] TASK 19 — Rules workspace: list, search, and filters

**Dependencies**: TASK 18.

**Files to touch**:
- `src/lib/components/Rules.svelte` (new)
- `src/lib/components/EmptyState.svelte` (reuse, edit only if a variant is needed)
- `src/lib/styles/tokens.css` (only if a token is genuinely missing)

**Exact requirements** (spec):

> "Catalog of available rules"

> "Filters by target tool, category, etc."

> Rule list: search plus filters by harness and by category.

> Loading, empty, error, and partial states are all rendered (ENG-19).

> No inline styles except for dynamic values; use `src/lib/styles/tokens.css`.

**Acceptance criteria**:
- Search matches on `name`, `description`, and `slug` (case-insensitive).
- Two filter controls: **harness** (only installable harnesses, from
  `ruleHarness`/`installableHarnesses` — never a literal list) and **category**
  (derived from the loaded rules, sorted).
- Filters and search compose (AND), and a "clear filters" affordance appears
  only when something is active.
- Five distinct states, each rendered: loading, error (message + retry), empty
  because **not synced**, empty because the catalog **ships no rules**, and
  populated. The two empty states must have different copy and different CTAs
  (spec §8).
- Invalid rules are visible with an error affordance (badge/icon + accessible
  label) and are not silently hidden; the list never claims a rule is
  installable when `validation.valid` is false.
- Layout mirrors `Runbooks.svelte`'s structure (`.rbv` shell, header with title +
  subtitle, scrolling body) so the app reads as one product; styles use tokens
  only.
- Accessibility: the list is a real list, the filter controls have labels,
  keyboard reachable, visible focus, contrast ≥ 4.5:1 (ENG-23).
- No rule is fetched per row on mount (no N+1): the list payload carries what it
  renders; the body is fetched on selection (TASK 20).

**QA gate**: `mise run check` + `mise run build-frontend` green. Reviewer drives
the five states with a stubbed store: each state visually distinct, the
not-synced CTA differs from the no-rules message, and `/` focuses the search
field (existing global behaviour in `+page.svelte`).

---

### [ ] TASK 20 — Rule detail panel and compiled preview

**Dependencies**: TASK 18, TASK 19.

**Files to touch**:
- `src/lib/components/RuleDetail.svelte` (new)
- `src/lib/components/Rules.svelte`

**Exact requirements** (spec):

> Detail panel: neutral body preview, frontmatter summary, validation result.

> Compiled preview: switch between the four harnesses and see the exact bytes
> that would be written, with the destination path shown.

> "Distinctive icon for rules vs agents"
> *(the sidebar mark is TASK 22; the detail panel must not reuse the agent `Bot` icon to mean "rule")*

**Acceptance criteria**:
- Neutral panel shows: name, description, category, and the frontmatter summary
  (globs, alwaysApply, trigger, match, tags, tools — each omitted when absent),
  plus the markdown body rendered as text (monospace `<pre>` is acceptable;
  no markdown renderer is in scope and none may be added).
- Validation result is explicit: a clear pass state, or the verbatim error
  strings from `RuleValidation.errors` (no reformatting — spec §8).
- Harness switch is a real tablist (roving focus or a radiogroup), with exactly
  the installable harnesses as options; the selected harness's **exact bytes**
  render in a `<pre>` with the destination path shown above it.
- Scope switch (user / project) sits with the harness switch; selecting project
  requires a root before a dest can be shown, and the panel says so instead of
  showing a user path (spec §7).
- Bytes are displayed exactly as they will be written — no trimming, no
  syntax-highlighting that alters characters in the copy path. A "copy" action
  copies the exact bytes.
- Switching harness does not refetch the corpus; switching scope with a new
  project root may refetch the preview (cache key includes the root).
- Accessibility: the tablist has `role="tablist"`/`role="tab"`/`aria-selected`,
  panels have `role="tabpanel"`, and the path is `aria-describedby`-linked to the
  bytes region.

**QA gate**: `mise run check` and manual verification: for `typescript`, the
antigravity preview shows `globs: "**/*.ts,**/*.tsx"`, the oh-my-pi preview shows
`alwaysApply: false` for a rule that sets it, and the OpenCode preview has no
`name:` line — compared against
`/home/deuriib/projects/agency-rules/integrations/`. Invalid rule shows its
verbatim error and no bytes.

---

### [ ] TASK 21 — Install and uninstall controls

**Dependencies**: TASK 16, TASK 17, TASK 18, TASK 20.

**Files to touch**:
- `src/lib/components/RuleDetail.svelte`
- `src/lib/components/Rules.svelte`
- `src/lib/stores/rules.svelte.ts` (only if a loading/error field is missing)

**Exact requirements** (spec):

> "Per-rule deployment controls"

> Install controls: per-rule, per-harness, with scope selection where the
> harness supports it (user / project). Project scope requires a project root
> the user picks — no silent writes to an arbitrary directory.

> Uninstall must be available for anything the app installed.

> The install is visible in the app after a restart (ledger is the source of
> truth, not component state).

**Acceptance criteria**:
- Install action is per (rule, harness, scope). It is disabled with a **stated
  reason** when: the rule is invalid, the harness is not installable, or project
  scope is selected without a root. Disabled-without-a-reason is a defect.
- Project root comes from a user action (the `@tauri-apps/plugin-dialog`
  directory picker already used elsewhere in the app — reuse it). It must **not**
  create a Projects-registry entry: §3 keeps "Assigning rules to Projects" in M2.
- After a successful install: the row shows the installed state from
  `rules_installs` (re-read, not optimistically fabricated), and a toast
  confirms. After a failure: the verbatim error appears, and the state does not
  claim success.
- Uninstall appears for every (rule, harness, scope) present in
  `rules_installs`, plus the project path when the scope is project.
- Install state visible after an app restart with no UI interaction beyond
  loading the section (a ledger-driven read).
- Buttons show in-flight state and cannot double-submit (`aria-busy` /
  `disabled` while pending).
- All strings come from `i18n.t(...)` with interpolation, never concatenation.

**QA gate**: manual + automated evidence in the PR: install `coding-standards`
into antigravity (user), verify the file at
`~/.gemini/config/rules/coding-standards.md` is byte-identical to preview,
restart the app, confirm the installed badge persists; uninstall, confirm the
file and the ledger row are gone. Project-scope install into a temp dir writes
under that dir and creates no Projects entry (`projects_list` unchanged).
Attempting project-scope install with no root never touches the filesystem.

---

### [ ] TASK 22 — Sidebar entry, section switch, and palette shortcut

**Dependencies**: TASK 19.

**Files to touch**:
- `src/lib/components/Sidebar.svelte` (`nav`, `label()`)
- `src/routes/+page.svelte` (`sectionTitle()`, the section `{#if}` chain, the
  `Cmd+0..N` keydown map)
- `src/lib/types.ts` (`SidebarSection`)
- `src/lib/components/CommandPalette.svelte` (command entry)
- `src/lib/components/Rules.svelte` (page header title, if the pattern requires)

**Exact requirements** (spec):

> New `Rules` tab in main navigation.
> "Distinctive icon for rules vs agents"

> New sidebar entry **Rules**, distinct icon from Agents (`Bot`), with an i18n
> label; keyboard shortcut must be announced in the palette and must not collide
> with existing 0–6 bindings.

> | `src/lib/components/Sidebar.svelte:28-36` | `nav: NavItem[]` | Add the Rules entry; shortcut assignment must not collide with 0–6. |
> | `src/routes/+page.svelte:39-60,234` | `SidebarSection` switch | Add the `rules` branch rendering the new workspace. |

**Acceptance criteria**:
- `SidebarSection` gains `"rules"`; the `nav` array gains
  `{ id: "rules", shortcut: shortcut("7"), icon: <not Bot> }`.
- Shortcut is **7** — `0`–`6` are all taken (`dashboard`, `personas`, `tools`,
  `teams`, `projects`, `runbooks`, `activity`) and `L` is the drawer. The
  `+page.svelte` keydown map and its `["0"…"6"]` guard are extended to include
  `"7"`.
- The icon is a distinct lucide glyph from `Bot` (`BookMarked`, `Ruler`,
  `ScrollText` — any glyph that is not `Bot` and not already used by another nav
  item); a comment next to it states why it is distinct from the Agents mark.
- `label()` and `sectionTitle()` gain a `rules` branch using
  `i18n.t("nav.rules")`.
- `+page.svelte`'s section chain gains `{:else if ui.section === "rules"} <Rules />`,
  imported with the other components.
- `CommandPalette.svelte` gains a `rules` command with
  `shortcut("7")` and a `palette.openRules` label, in the `palette.nav` group —
  i.e. the shortcut is announced, not just bound.
- The sidebar footer's agent count stays agent-only (rules are not agents and
  M1 adds no rule metrics — §3 keeps Dashboard metrics for M2).

**QA gate**: `mise run check` + manual: navigation via sidebar click, via
`Cmd/Ctrl+7`, and via the palette all reach the Rules workspace; the palette row
shows the shortcut; no existing `0`–`6` binding changed (assert by pressing each
and landing on the same section as before).

---

### [ ] TASK 23 — i18n: every new string in all 12 locales

**Dependencies**: TASK 19, TASK 20, TASK 21, TASK 22.

**Files to touch**:
- `src/lib/i18n/locales/en.ts` (source of truth for `MessageKey`)
- `src/lib/i18n/locales/de.ts`
- `src/lib/i18n/locales/es.ts`
- `src/lib/i18n/locales/fa.ts`
- `src/lib/i18n/locales/fr.ts`
- `src/lib/i18n/locales/ja.ts`
- `src/lib/i18n/locales/ko.ts`
- `src/lib/i18n/locales/pt-BR.ts`
- `src/lib/i18n/locales/ru.ts`
- `src/lib/i18n/locales/tr.ts`
- `src/lib/i18n/locales/zh-CN.ts`
- `src/lib/i18n/locales/zh-TW.ts`
- `scripts/check-i18n.mjs` (new)
- `scripts/check-i18n.test.mjs` (new)

**Exact requirements** (spec):

> i18n: all new user-facing strings in all 12 locale files.

> All new strings present in the 12 locale files (`de`, `en`, `es`, `fa`, `fr`,
> `ja`, `ko`, `pt-BR`, `ru`, `tr`, `zh-CN`, `zh-TW`).

**Acceptance criteria**:
- Every string introduced by TASK 19–22 exists in **all twelve** files listed
  above, named individually — `en` plus `de`, `es`, `fa`, `fr`, `ja`, `ko`,
  `pt-BR`, `ru`, `tr`, `zh-CN`, `zh-TW`. "Eleven other locales" is not
  acceptable; count them.
- Keys live under a `rules.*` namespace (`rules.*`, plus `nav.rules` and
  `palette.openRules`), matching the existing flat dotted convention in `en.ts`.
- Interpolations use the existing `{name}` placeholder form; no string
  concatenation at call sites (ENG-30).
- `fa.ts` is RTL — no string may rely on positional punctuation that breaks in
  RTL (no leading ":" or "/" glued to a label).
- `scripts/check-i18n.mjs` exports `missingKeys(baseKeys, localeKeys)` and
  `extractKeys(sourceText)`, and its test asserts that every `rules.*` /
  `nav.rules` / `palette.openRules` key present in `en.ts` is present in each of
  the eleven other files. **Scope the guard to the new key namespaces**: the
  locale files are `Partial<Messages>` today and may have pre-existing gaps;
  a guard over all keys would fail for reasons unrelated to this PR. Log any
  pre-existing gap found in the PR description as a separate follow-up, do not
  fix it here.
- Runs under `npm run test:scripts` (existing `node --test scripts/*.test.mjs`
  glob) — no new tooling.

**QA gate**: `npm run test:scripts` green. Reviewer greps one new key in all
twelve files and confirms a real translation (not the English string pasted into
`de`, `ja`, `ru`, …). `mise run check` green (proves the `MessageKey` union
still type-checks).

---

### [ ] TASK 24 — Docs: README behaviour section (DOC-01)

**Dependencies**: TASK 16, TASK 21, TASK 22.

**Files to touch**:
- `README.md` (`## Features` — new `### Rules` block; `## Architecture` if it
  enumerates modules)
- `docs/BUILD.md` (the `rules-parity` note from TASK 10, next to the existing
  parity instructions)

**Exact requirements** (spec):

> Docs: `README.md` behaviour section updated in the same PR (DOC-01).

> Behavior, flag, installed path, convention, or structure change ships with its
> nearest `README.md` / `PRODUCT.md` / `AGENTS.md` / `docs/**/*.md` update in the
> same commit/PR — no "docs later". *(DOC-01)*

**Acceptance criteria**:
- `README.md` `## Features` gains a `### Rules` block stating, in the user's
  terms: a Rules catalog backed by the managed `agency-rules` corpus; filters by
  harness and category; neutral-body and compiled preview per harness with the
  destination path; per-rule per-harness install with user or project scope;
  installs tracked in the same local ledger as agents; uninstall available for
  anything the app installed.
- The block explicitly names the **four harnesses** (Google Antigravity,
  pi-agent, oh-my-pi, OpenCode) and states that M1 does **not** include rule
  drift detection, rule reconciliation, rule auto-update, per-project rule views,
  or a custom-rules directory (§3's out-of-scope list, stated as a limitation —
  the same honesty the README already applies in `## What This Isn't`).
- **The rule harnesses must NOT be added to the `## Supported Install Targets`
  table.** `scripts/check-doc-tools.mjs` derives that table from
  `tools.json` ∩ `render-formats.json`; rule harnesses are a separate contract
  and adding rows would fail the guard. Use a separate subsection.
- `docs/BUILD.md` documents how to run the `--ignored` rules-parity test,
  mirroring the existing `mise run parity` paragraph.
- No drive-by rewrites (DOC-05): only the touched sections change.
- `docs/rules-integration-spec.md` is **not** rewritten. If implementation
  revealed a spec error, add a dated note at its end under a
  `## Implementation notes` heading; do not edit the locked decisions.

**QA gate**: `npm run test:scripts` green (this includes the doc-tools guard —
proving the README table is still truthful). A reviewer reads only the README and
can answer: where do rules come from, where do they install, what is tracked, and
what is explicitly not supported in M1.

---

### [ ] TASK 25 — Regression: agent functionality and old-ledger parsing unchanged

**Dependencies**: TASK 13, TASK 16, TASK 17.

**Goal**: Prove the load-bearing claim in spec §10.8 — this milestone is
additive. Run at the end, after all rules code is in.

**Files to touch**:
- `src-tauri/tests/rules_regression.rs` (new)
- `src-tauri/tests/fixtures/installs-v0.8.x.json` (from TASK 13, extended if
  needed)

**Exact requirements** (spec):

> Agent functionality is unchanged: agent install, reconcile, and the Library
> still behave exactly as before, and a pre-existing ledger still parses.

> a ledger written by v0.8.x must still parse, and agent rows must be
> byte-identical on re-serialize. Rules must not appear in agent reconciliation
> output.

**Acceptance criteria**:
- The full pre-existing Rust suite (`cargo test --lib --tests`) passes with no
  test removed, renamed-to-skip, or `#[ignore]`d. Test count must not decrease;
  state the before/after count in the PR description.
- Agent render output is byte-identical: the existing
  `upstream_convert_sh_is_byte_identical_for_transform_tools` `--ignored` parity
  test is re-run against a real `agency-agents` checkout (or its committed
  equivalent) and matches.
- Old-ledger round-trip: the v0.8.x fixture parses, backfills `dests`, and
  re-serializes **byte-identically**.
- Mixed-ledger behaviour: the fixture plus injected rule rows → agent install,
  uninstall, update, and reconcile behave exactly as with the fixture alone.
- No rules symbol leaks into the agent surface: `installs_reconcile` output
  contains no rule slug; `corpus_list`/`corpus_get` are unaffected;
  `runbooks_list` is unaffected.
- Rules code cannot write into the agent corpus root: a test asserts
  `rules_corpus_dir` and `corpus_dir` are different paths for the same app-data
  dir, and that a rules install never touches the agent corpus tree.

**QA gate**: `mise run ci` green end-to-end (fmt-check, clippy, test-rust,
test-scripts, check, build-frontend), PR description carries: Rust test counts
before/after, the 28/28 conformance result, the twelve-locale check, and the
manual install→restart→uninstall transcript.

---

## QA Gates

Every task in this document must pass, on the branch, before it can be merged:

1. **`mise run ci` is green.** That is exactly what
   `.github/workflows/pr-check.yml` runs: `fmt-check` (rustfmt), `clippy`
   (`-D warnings`), `test-rust` (`cargo test --lib --tests`), `test-scripts`
   (`node --test scripts/*.test.mjs`), `check` (svelte-check), and
   `build-frontend` (`npm run build`). A red or unrun suite is not done.
2. **The task's own tests exist and pass.** Every new Rust module ships unit
   tests in the same task; renderers additionally ship golden fixtures (TASK 10);
   no task may "add tests later".
3. **No gate is weakened to pass.** No `#[ignore]` added to an existing test, no
   `--no-verify`, no `@ts-ignore`, no `--all-features`-style workaround, no
   skipping a locale, no editing a golden fixture to match a renderer
   (DENY-04, ENG-04).
4. **Spec §9 gates hold**: no `any`; strict TypeScript; new Rust logic covered;
   all 12 locale files updated; `README.md` behaviour section updated in the same
   PR; Conventional Commits; no direct-to-main.
5. **Scope discipline**: the task implements only what its "Exact requirements"
   block quotes. Anything from the Out-of-scope list appearing in a diff is a
   review blocker, not a nice-to-have.

---

## Orchestrator rulings on the ambiguities

Ruled by AgentsOrchestrator on 2026-11-03. These override the task text above
where they conflict; the dev agents implement the ruling, not the original text.

| ID | Ruling | Note |
| --- | --- | --- |
| A1 | **Approved** | `src-tauri/src/rules/{registry,parse,validate,render,corpus,commands}.rs` behind `rules/mod.rs`, matching the `corpus/` precedent. |
| A2 | **SUPERSEDED by #68** | `{{slug}}` was honoured verbatim; issue #68 unified the placeholder to `{slug}` and inverted the guard — a double-brace template is now the rejected form. The no-residual-placeholder test remains mandatory. |
| A3 | **Approved** | Slug = frontmatter `name`. Proven by the fixtures, not assumed. |
| A4 | **Approved** | Corpus lands at TASK 11–12. |
| A5 | **Approved** | `rules_status` and `rules_installs` are required to satisfy spec §8 and §10.6. |
| A6 | **OVERRULED — implement it** | Registering the `opencode-rules` plugin is part of a working OpenCode install, not a follow-up. See "A6 addendum" below. |
| A7 | **SUPERSEDED by #68** | The `kebab` lookup existed only because the two id schemes disagreed. Harness ids are now the `tools.json` ids, so `tool_for_harness` is an existence check and `settings.tool_paths` is still honoured. See "A7 addendum" below. |
| A8 | **Approved** | Delete recorded files only; never prune directories. |
| A9 | **Approved** | `project_path: Option<String>`; the response reports the scope rendered. |
| A10 | **Approved** | Document the `tools`/`match` fixture gap in the test header; cover both with unit tests. |
| A11 | **Approved** | Promote `jsonschema` to a runtime dependency. |
| A12 | **Approved** | The i18n guard asserts parity for the new namespaces only. |
| A13 | **Approved** | Purpose-built YAML emitter over the fixed field set, with the reason in code. |
| A14 | **Approved** | Env-gated parity re-check plus the `docs/BUILD.md` note. |

### A6 addendum — OpenCode plugin registration (new TASK 16b)

Without an `opencode-rules` entry in `opencode.json`, a rule written to
`.opencode/rules/` is inert. Installing it would meet the letter of §10.5 while
failing its intent. Implement, tightly scoped:

- Trigger: only when installing a rule for harness `opencode`. No other harness
  touches `opencode.json`.
- Merge-safe: read the file; if the `plugin` array already contains any entry
  whose string contains `opencode-rules`, write nothing.
- Otherwise append the contract's `plugin.entry` value from the rules contract
  (do not hardcode `opencode-rules@latest` in code) and write atomically.
- Never remove, reorder, or rewrite unrelated keys. Preserve 2-space indent.
- Fail closed: unreadable or unparseable `opencode.json` aborts the install with
  a specific error and **zero** filesystem writes — not a silent skip, not a
  clobbered config.
- Uninstall removes the rule file and the ledger row but **never** the plugin
  entry (other rules may depend on it). State this in the uninstall confirmation
  copy.
- Target file: project scope → `<project_root>/opencode.json`; user scope →
  `~/.config/opencode/opencode.json` (both from the contract's `detect`/home
  conventions). If the file does not exist, create it with only the `plugin`
  key.

### A7 addendum — harness → tool mapping (superseded by #68)

Originally: `agency-rules` harness ids equalled the `kebab` field in
`src-tauri/data/tools.json` (`pi-agent` → `piAgent`), so the resolver compared
against `kebab` instead of adding a second table.

Issue #68 removed the mismatch at the source: `agency-rules` v0.10.0 uses the
agent tool ids themselves (`piAgent`, `ohMyPi`), so the harness id **is** the
registry id and `tool_for_harness` is an existence check. The behavioural
requirements are unchanged:

- User-scope destinations expand `~/` against `settings.tool_paths[tool_id]`
  when that override is set, else the OS home.
- A harness the registry does not declare falls back to the OS home and the
  resolver stays total (no panic, no error).
- This must not change any resolved agent path. The regression task (TASK 25)
  proves it.
- Ledger rows written before the rename still uninstall: `parse_ledger` maps
  `pi-agent → piAgent` and `oh-my-pi → ohMyPi` at read time.

---

## Orchestrator rulings — ArchitectUX review (C1–C10)

Ruled 2026-11-03. Source: `docs/rules-integration-architecture.md` and
`docs/rules-integration-ux.md`. These are binding.

| ID | Ruling |
| --- | --- |
| C1 | **A6 wins.** The "Do not write `opencode.json`" sentence in TASK 8 is void. The OpenCode renderer stays pure; only `rules/opencode.rs` touches the config file. |
| C2 | **`dests` holds rule files only.** `opencode.json` is never a tracked destination. `rules_uninstall` must not be able to delete a user config. The plugin registration is reported as a separate boolean on the install result, never as a deletable path. |
| C3 | **Rust stays `InstallRecord`; TypeScript aliases it as `RuleInstall`.** One wire type, no parallel shape. |
| C4 | **Every agent-facing ledger consumer must filter rule rows.** Extend TASK 13 to cover `tools_list` (`install/mod.rs:1118`), `projects_list` (`:1234`), and `loadout_export` / `loadout_import` (`:1281`), not just `installs_reconcile` and `installs_for_agent`. Add an explicit regression test per surface. Rationale: the rule rows put a harness id in `tool`, and `antigravity`/`opencode` are byte-equal between the two id schemes, so an unfiltered consumer double-counts silently. |
| C5 | **REVISED — fixtures are 8 rules × 4 harnesses = 32, generated from a fresh reference run.** QA proved the checked-in `agency-rules/integrations/` tree is stale (generated at `1c846f5`; `agentmemory` drifted in `82befdc`/`bde758c`, and `mcp-gateway-protocol` compiles but was never regenerated there). Copying it would fail 4/32 assertions against a *correct* renderer. TASK 10 therefore regenerates the oracle with the sibling CLI into a temp dir and vendors that, recording the source commit and the exact command in the fixture header. The env-gated parity test keeps C5's original rule: a missing upstream artifact is a **reported finding, not a failure**. |
| C6 | **`RuleEntry` gains `relative_path`.** Slug ≠ filename in the real corpus, so `rules_get` cannot locate a rule without it. |
| C7 | **`always_apply` is `Option<bool>`.** Encoding it as `bool` collapses `false` and absent, which the oh-my-pi renderer needs to distinguish. |
| C8 | **A5 is extended: `rules_refresh` is approved.** Without it the `notSynced` empty state has no action. |
| C9 | **The A7 fallback branch needs a synthetic test.** For the shipped contract every harness resolves, so the branch is otherwise untested. |
| C10 | **A7 supersedes ambiguity #7.** Harnesses honour `settings.tool_paths`. Record the supersession in the PR body so nobody reverts it. |
| C11 | **Multi-line values fail closed.** A schema-valid rule can carry a multi-line `description`, and the reference compiler emits a folded block (`>`) plus block-end trailing-space trimming that this emitter does not reproduce. `render::render_blockers` reports those fields, and `rules_preview` / `rules_install` MUST refuse such a rule with the blocker message instead of writing bytes that differ from upstream. Implementing `>` folding is a follow-up; the guard stays until then. |

Two implementation notes accepted as constraints:

- `serde_json` needs the `preserve_order` feature so the `opencode.json` merge
  cannot reorder a user's keys. The old-ledger byte test is the proof that
  enabling it changes nothing else (TASK 13).
- The `nexus-strategy` fixture is the only one exercising the 80-column fold, so
  the emitter's own unit tests — not the fixtures — are the real gate. TASK 5
  must test folding directly.
- The reference `integrations/` tree is generated output and is gitignored
  upstream. Never treat it as a checked-in oracle; regenerate it in TASK 10.

---

## Workstreams (execution order for the dev-QA loop)

Tasks are grouped into independently shippable workstreams. QA validates every
task's acceptance criteria inside its workstream before the next one starts.

| WS | Tasks | Theme |
| --- | --- | --- |
| W1 | 1, 2, 3, 4 | Contract + parse + validate |
| W2 | 5, 6, 7, 8 | The four renderers |
| W3 | 9, 10 | Render entry point + conformance |
| W4 | 11, 12 | Rules corpus |
| W5 | 13, 16b | Ledger discriminator + OpenCode plugin merge |
| W6 | 14, 15, 16, 17 | Command surface |
| W7 | 18, 19, 20, 21, 22 | Frontend store + Rules workspace |
| W8 | 23, 24, 25 | i18n + docs + regression |

---

## Ambiguities and judgement calls

Recorded so the orchestrator can confirm or overrule. Each is already reflected
in the task text above. **See "Orchestrator rulings" immediately above — it
supersedes this section where they differ.**

1. **Module layout (A1).** The spec names anchors but not files. Chosen:
   `src-tauri/src/rules/{registry,parse,validate,render,corpus,commands}.rs`
   behind `src-tauri/src/rules/mod.rs`, following the `corpus/` and `install/`
   precedent of commands living in their domain module rather than
   `src-tauri/src/commands/`. Alternative: one flat `rules.rs`.
2. **Placeholder syntax (A2).** The mirrored contract uses `{{slug}}` **double**
   braces; the app's agent `tools.json` uses `{slug}` **single**. Chosen: honour
   `{{slug}}` verbatim and assert no `{{` survives resolution. Not stated in the
   spec; derived from `agency-rules/tools.json`.
3. **Slug source (A3).** Spec §5/§8 say slug is schema-constrained frontmatter,
   and `RULE-SPEC.md` says the filename "should equal" the frontmatter `name`.
   The corpus violates "should" (`rules/base/commit-convention.md` →
   `name: commit-conventions`), and the compiled artifacts follow the
   *frontmatter* name. Chosen: **slug = frontmatter `name`**, filename ignored.
   This is proven by the fixtures rather than assumed, but it is a call the spec
   does not state outright.
4. **Corpus task position (A4).** The requested order is "data contract → Rust
   parsing/validation → Rust renderers → ledger → commands → …". Spec §3 item 1
   requires the rules corpus, which the requested order does not mention.
   Inserted as TASK 11–12, after conformance and before the ledger, because
   commands depend on it and renderers do not.
5. **`rules_status` / `rules_installs` are extra commands (A5).** Spec §3.5 lists
   "list/get/preview/install/uninstall". §8's "distinguishes 'not synced' from
   'no rules shipped'" and §10.6's "visible after a restart (ledger is the source
   of truth)" cannot be satisfied without a status read and a ledger read, so two
   small commands were added to TASK 12 and TASK 17. Confirm they are acceptable
   rather than folding their data into `rules_list`.
6. **OpenCode plugin registration (A6).** `rules-tools.json` carries a `plugin`
   block (`opencode-rules@latest`) that upstream's installer writes into
   `opencode.json`. Spec §7 says the compiled preview shows "the exact bytes that
   would be written" and §3.5 lists no plugin-registration command, so M1 writes
   the rule file only and never mutates `opencode.json`. If the user must register
   the plugin for the rules to take effect, that is a real functional gap — flag
   for confirmation, do not implement speculatively.
7. **Per-tool home overrides do not apply to harnesses (A7).** Agents resolve
   user scope through `settings.tool_paths[tool]`. Rule harness ids
   (`antigravity`, `pi-agent`, `oh-my-pi`, `opencode`) are a separate contract and
   are deliberately not mapped onto agent tool ids (`antigravity`, `piAgent`,
   `ohMyPi`, `opencode`). Chosen: expand `~/` against the OS home only. If the
   mapping is wanted, it is a spec change, not an implementation detail.
8. **Pi-agent uninstall leaves the empty directory (A8).** §10.7 says "removes the
   file(s)". Chosen: delete recorded files only, never prune directories.
9. **`rules_preview` scope (A9).** Preview needs a project root to resolve a
   project dest, which does not exist until the user picks one. Chosen:
   `project_path: Option<String>`; `None` previews user scope and the response
   reports which scope it rendered.
10. **Fixture coverage gap (A10).** No rule in the corpus exercises `tools` or
    `match`, so the golden fixtures cannot cover them. Chosen: fixtures cover the
    7 real rules × 4 harnesses (28 comparisons); `tools`/`match` are covered by
    renderer unit tests only, and the gap is documented in the test header.
11. **Schema validation dependency (A11).** `jsonschema` is currently a
    dev-dependency (used by `tests/tools_schema.rs`). Validation is now a runtime
    concern, so TASK 4 promotes it. Alternative: hand-roll the eight-field
    schema check — rejected because §3.2 says "validated against the shipped JSON
    Schema", which implies real schema validation.
12. **i18n guard scope (A12).** Locale files are `Partial<Messages>` and may
    already have gaps. The new guard asserts parity only for the newly added
    namespaces; a pre-existing gap is reported as a follow-up, not fixed in this
    PR. Confirmed against `src/lib/i18n/messages.ts` (`{ ...en, ...overrides }`).
13. **YAML emission strategy (A13).** The reference uses the `yaml` npm package.
    A general Rust YAML serializer (`serde_yaml`) emits block sequences without
    the 2-space indent the reference produces, so conformance would fail on every
    globbed rule. Chosen: a small purpose-built emitter over the fixed field set,
    with the reason recorded in code. This is an implementation constraint the
    spec's "byte-for-byte" requirement forces but does not itself state.
14. **Parity re-check test (A14).** TASK 10 adds an `#[ignore]`d,
    env-gated staleness test (`AGENCY_RULES_PARITY_ROOT`) plus a `docs/BUILD.md`
    note, mirroring `mise run parity`. Not required by the spec; included because
    static fixtures rot and `decisions.md` 2026-06-14 already establishes parity
    as "a tested contract, not an assumption". Drop it if considered scope creep.
