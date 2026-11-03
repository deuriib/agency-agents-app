# Rules Integration — Build Spec (Issue #29, Milestone M1)

**Status**: Approved for implementation (decisions locked 2026-11-03)
**Issue**: [#29 — feat/Integrate agency-rules for unified AI rules management](https://github.com/deuriib/agency-agents-app/issues/29)
**Branch**: `feat/29-rules-integration`
**Scope**: M1 only — Rules catalog, compiled preview, install, ledger tracking

---

## 1. Context

`agency-agents-app` already manages the `agency-agents` catalog: browse, install
across tools, track installs in a local ledger, detect drift, reconcile, and
auto-update. The sibling project [`deuriib/agency-rules`](https://github.com/deuriib/agency-rules)
solves the same problem for **AI rules** — write a rule once in neutral Markdown
and compile it to four harness formats.

Issue #29 proposes folding that capability into the app so a user manages agents
and rules from one surface. This spec fixes the open technical questions from the
issue and defines the M1 build.

### 1.1 Locked decisions

| Question (from issue #29) | Decision | Rationale |
| --- | --- | --- |
| Integration strategy (A dependency / B native / C plugin) | **B — native Rust port** | `systemPatterns.md §3` already establishes this contract for agents: "done natively in Rust (no shell/python at runtime) … `scripts/convert.sh` is the *reference spec*, not a runtime dependency." Option A would bundle a Node runtime into a Tauri app; Option C has no concrete pain point to justify the abstraction (ENG-13). |
| Milestone scope | **M1 only** | See §3. Reconciliation, Projects, and Dashboard metrics are deferred to M2. |
| Rules catalog source | **Corpus-copy of `deuriib/agency-rules`** | Mirrors `systemPatterns.md §1` exactly: bundled baseline → extract to app-data → refresh from the GitHub tarball → rebuild an index. No runtime git dependency. |
| Central catalog vs user-managed rules | **Central catalog** (corpus-copy). A user-local custom-rules directory is deferred to M2. | Discoverability is the stated benefit in the issue; a read-only catalog is the smaller, verifiable slice. |
| Neutral vs native format | **Neutral format is the only authored format.** Each rule exists once under `rules/<category>/<slug>.md`; the four harness variants are compiled, never authored. | This is already true in `agency-rules` (`docs/RULE-SPEC.md`); the app must not introduce a second source of truth. |
| Validation before install | **Required** — frontmatter is validated against the schema before any compile or write. | Issue asks for it explicitly; it is also the only way to fail closed instead of writing a broken rule file. |

---

## 2. Requirements quoted from issue #29

### 2.1 New "Rules" Workspace (similar to the Agents workspace)

- "Catalog of available rules"
- "Filters by target tool, category, etc."
- "Detail panel with content preview"
- "Per-rule deployment controls"

### 2.2 Compiler Integration

- "Use `agency-rules` as the backend for rule compilation"
- "Support the declarative format (`tools.json`)"
- "On-demand or pre-compiled generation"

### 2.3 Tracking and Reconciliation

- "Extend the local ledger to include rules" ← **M1**
- "Detect drift in rule files" ← M2
- "Reconciliation similar to agents" ← M2

### 2.4 Rule Formats

`agency-rules` supports, and the app must maintain compatibility with all four:

| Harness | Format | Project destination | User destination |
| --- | --- | --- | --- |
| Google Antigravity | `md-yaml-trigger` | `.agents/rules/{{slug}}.md` | `~/.gemini/config/rules/{{slug}}.md` |
| pi-agent | `md-rules-dir` | `.pi/rules/{{slug}}/RULES.md` | `~/.pi/agent/rules/{{slug}}/RULES.md` |
| oh-my-pi (omp) | `md-or-mdc` | `.omp/rules/{{slug}}.md` | `~/.omp/agent/rules/{{slug}}.md` |
| OpenCode | `md-plugin` | `.opencode/rules/{{slug}}.md` | `~/.config/opencode/rules/{{slug}}.md` |

Source of truth for the table above: `agency-rules/tools.json` (`harnesses.*.dest`).
The app must read an equivalent declarative contract — it must **not** hardcode
destinations in render code (see `registry.rs:12-14`: "never restate this list").

### 2.5 UI/UX

- "New 'Rules' tab in main navigation"
- "Distinctive icon for rules vs agents"
- "Allow preview of how the compiled rule will look"
- "Support frontmatter validation"

Deferred to M2: "Combined filters (agents + rules) in Projects", "Updated Dashboard
showing health metrics for both".

---

## 3. M1 Scope

### In scope

1. **Corpus**: `deuriib/agency-rules` becomes a second managed corpus with its own
   baseline seed, tarball refresh, and `rules-index.json`.
2. **Rule parsing + validation**: neutral frontmatter parsed into a typed
   `Rule`; validated against the shipped JSON Schema before compile/install.
3. **Native renderers**: four deterministic Rust renderers (one per harness) that
   produce byte-identical output to the `agency-rules` reference renderers.
4. **Ledger**: rule installs recorded in the existing ledger file, distinguishable
   from agent installs, and reversible (install/uninstall).
5. **Commands**: a minimal Tauri command surface for list/get/preview/install/
   uninstall of rules.
6. **UI**: a Rules workspace reachable from the sidebar — rule list with filters
   (harness, category), detail panel with the neutral body, compiled preview
   per harness, and install controls.
7. **i18n**: all new user-facing strings in all 12 locale files.

### Explicitly out of scope (M2+)

- Drift detection and the five-state reconciliation for rules.
- Assigning rules to Projects; the unified Projects view.
- Dashboard health metrics for rules.
- User-local custom rules directory.
- Auto-update of installed rules.
- Teams/loadouts that include rules.
- Aggregated roster installs (the `_manifest.json` bundles are noted but not used).

---

## 4. Architecture anchors (reuse, do not reinvent)

| Anchor | Role | How rules extend it |
| --- | --- | --- |
| `src-tauri/src/registry.rs:12-14` | Embedded `tools.json` + `render-formats.json`, single source of truth | Add a parallel declarative contract for rule harnesses and a matched pair of Rust + TS readers, exactly like `implemented_formats()`. |
| `src-tauri/src/render/mod.rs:317-530` | `RenderedFile`, `render_files`, `render_with_hash`, `dests`, `sha256_hex` | Reuse `RenderedFile` and `sha256_hex`; add rule renderers beside the agent ones. |
| `src-tauri/src/install/mod.rs:38-71` | Ledger path + read/write with atomic serialization | Extend the record type rather than adding a second ledger file. |
| `src-tauri/src/types.rs:196` | `InstallRecord` | Add an optional discriminator so old ledgers keep parsing (`#[serde(default)]`). |
| `src-tauri/src/corpus/mod.rs:491,1471-1864` | `resolve_active`, `corpus_*`, `catalog_*`, `runbooks_list` | `runbooks_list` is the precedent for exposing a second content type from the catalog root. |
| `src-tauri/src/lib.rs` `invoke_handler` | Command registration | Register rule commands alongside `corpus::*` / `install::*`. |
| `src/lib/data/toolRegistry.ts:1-30` | TS mirror of `tools.json` | Rules need the same TS/Rust lockstep so the UI cannot offer an uninstallable target. |
| `src/lib/components/Sidebar.svelte:28-36` | `nav: NavItem[]` | Add the Rules entry; shortcut assignment must not collide with 0–6. |
| `src/routes/+page.svelte:39-60,234` | `SidebarSection` switch | Add the `rules` branch rendering the new workspace. |
| `src/lib/components/Runbooks.svelte` + `src/lib/stores/runbooks.svelte.ts` | Smallest existing example of a catalog-backed content section | Mirror its shape for the rules store and view. |

---

## 5. Data model requirements

1. `Rule` — the neutral rule: `slug`, `name`, `description`, `category`, `globs`,
   `alwaysApply`, `trigger`, `match`, `tags`, `tools`, plus `source_hash` and
   `body_hash` (body split from frontmatter, as agents do, so M2 can classify
   updates cosmetic vs substantive).
2. `RuleHarness` — one of `antigravity`, `pi-agent`, `oh-my-pi`, `opencode`.
3. Rule conformance is derived, never stored: a harness is installable iff a
   native renderer exists for its format.
4. Ledger rows gain a domain discriminator. Requirement: a ledger written by
   v0.8.x must still parse, and agent rows must be byte-identical on
   re-serialize. Rules must not appear in agent reconciliation output.
5. Backend and frontend must read the **same** declarative JSON for rule
   harnesses, formats, and destinations (the two-sides-cannot-drift rule from
   `registry.rs:12-14` and issue #49).

---

## 6. Renderer conformance (non-negotiable)

Each renderer must reproduce the reference output byte-for-byte, verified by
golden fixtures taken from `agency-rules/integrations/<harness>/`.

Known per-harness rules (from `agency-rules/docs/RULE-SPEC.md`):

- **Antigravity** — requires `trigger`. Precedence: explicit `trigger` →
  `alwaysApply: true` ⇒ `always_on` → `globs` present ⇒ `glob` → only
  `description` ⇒ `model_decision` → otherwise `manual`. When `trigger: glob`,
  `globs` is joined with **commas** into a single string.
- **pi-agent** — emits `name`, `description`, `tools`; drops `globs`,
  `alwaysApply`, `trigger`, `match`, `tags`. One directory per rule with
  `RULES.md` inside.
- **oh-my-pi** — emits `name`, `description`, `globs`, `alwaysApply`; drops the
  rest.
- **OpenCode** — emits `description`, `globs`, `match`; drops `name`,
  `alwaysApply`, `trigger`, `tags`, `tools`.

Determinism is REQUIRED: identical input must produce identical bytes so
`rendered_hash` is stable (same contract as agent renders).

---

## 7. UI requirements

- New sidebar entry **Rules**, distinct icon from Agents (`Bot`), with an i18n
  label; keyboard shortcut must be announced in the palette and must not collide
  with existing 0–6 bindings.
- Rule list: search plus filters by harness and by category.
- Detail panel: neutral body preview, frontmatter summary, validation result.
- Compiled preview: switch between the four harnesses and see the exact bytes
  that would be written, with the destination path shown.
- Install controls: per-rule, per-harness, with scope selection where the
  harness supports it (user / project). Project scope requires a project root
  the user picks — no silent writes to an arbitrary directory.
- Uninstall must be available for anything the app installed.
- Loading, empty, error, and partial states are all rendered (ENG-19).
- Accessibility: semantic controls, keyboard reachable, visible focus,
  contrast ≥ 4.5:1 (ENG-23, WCAG 2.1 AA).
- No inline styles except for dynamic values; use `src/lib/styles/tokens.css`.

---

## 8. Failure handling

- Invalid frontmatter → validation error surfaced verbatim, **no file written**.
- Unknown harness → never offered in the UI.
- Write failure → no partial file; report the path and the OS error.
- Missing corpus → empty state that distinguishes "not synced" from "no rules
  shipped", mirroring the lesson recorded in `decisions.md` 2026-11-02.
- Path traversal: `slug` comes from frontmatter and is schema-constrained to
  `^[a-z][a-z0-9-]*$`; re-validate in Rust before any path join (DENY-03).

---

## 9. Quality gates

Every task must pass, on the branch:

- `mise run ci` → `fmt-check`, `clippy`, `test-rust`, `test-scripts`, `check`,
  `build-frontend`.
- New Rust logic covered by unit tests; renderers covered by golden-fixture
  tests (TST-01, TST-03).
- No `any`, no `@ts-ignore`, strict TypeScript (ENG-01, ENG-04).
- All new strings present in the 12 locale files (`de`, `en`, `es`, `fa`, `fr`,
  `ja`, `ko`, `pt-BR`, `ru`, `tr`, `zh-CN`, `zh-TW`).
- Docs: `README.md` behaviour section updated in the same PR (DOC-01).
- Conventional Commits, no direct-to-main, reviewed before merge (ENG-26,
  DENY-12).

---

## 10. Acceptance criteria (M1)

1. A user can open a **Rules** section from the sidebar.
2. The section lists every rule in the managed `agency-rules` corpus, searchable
   and filterable by harness and category.
3. Selecting a rule shows its neutral content and its compiled output for each
   of the four harnesses, with the destination path.
4. An invalid rule is rejected with a visible, specific validation error and no
   filesystem write.
5. Installing a rule writes the correct bytes to the correct destination for the
   chosen harness and scope, and records the action in the ledger.
6. The install is visible in the app after a restart (ledger is the source of
   truth, not component state).
7. Uninstalling removes the file(s) the app wrote and the ledger row.
8. Agent functionality is unchanged: agent install, reconcile, and the Library
   still behave exactly as before, and a pre-existing ledger still parses.
9. `mise run ci` is green.
