# November 2026 — Antigravity parity + updater pipeline

## Current State
**v0.9.0** (`00b9576`) shipped the Rules integration (PR #61, merged `1ab9453`) on top
of the 0.8.x updater work. `main` is green. The v0.9.0 GitHub release is still a
**draft** with Linux artifacts only: `latest.json`, the Windows payloads and the
notarized macOS build are missing, so the updater cannot serve it yet.

## Goals
- [ ] Publish v0.9.0: signing secrets, local macOS release run, `verify-updater`.
- [ ] Triage the #61 review follow-ups (#62–#72). Suggested order: #72 (submodules) →
  #68 (rule vocabulary) → #62 (frontend tests) → #66 (workspace/category parity).
- [ ] Native review of the 12-locale Rules strings.

## Recent Achievements
- Made `src-tauri/Cargo.lock` track the release version (issue #81): a
  `cargo metadata --locked` gate in PR Check and `mise run check-lock`, plus
  `release-lock.yml` committing the sync into the release PR so the tagged commit
  matches its manifest. See [261103_cargo-lock-sync.md](./261103_cargo-lock-sync.md).
- Unified the Rules workspace with the Agents grammar (issue #66): one `WorkspaceShell`
  (resizable, persisted detail pane), one `CategoryMenu`, a category landing with an
  "all" destination, a category overview, and an install-state lens with counts. Categories
  are dressed by `rules-categories.json` with a neutral fallback, the duplicated `<h1>` and
  layout CSS are gone, and the suite grew to 58 tests. See
  [261103_rules-workspace-shell.md](./261103_rules-workspace-shell.md).
- Added the frontend test runner (issue #62): Vitest + `@testing-library/svelte` on jsdom,
  48 tests in 9.5 s, wired into `mise run ci` and PR Check. The Rules derivation moved to
  `src/lib/rules/derive.ts` so the install-state semantics are testable without a component,
  and four guards were proven by reverting each fix locally. See
  [261103_frontend-test-runner.md](./261103_frontend-test-runner.md).
- Unified the agent/rule vocabulary (issue #68): harness ids are now the agent tool ids
  (`piAgent`, `ohMyPi`) and every destination uses `{slug}`, in `agency-rules` v0.10.0 and
  in this repo. `tool_for_harness` collapsed to an existence check, the compiler refuses the
  legacy `{{slug}}` form, and a v0.9 ledger row still uninstalls through a read-time alias.
  See [261103_rule-vocabulary.md](./261103_rule-vocabulary.md).
- Pinned the catalog and rule contracts as git submodules (issue #72): `vendor/agency-agents`
  at v1.6.3 and `vendor/agency-rules` at v0.10.0, with `scripts/sync-vendor.mjs` generating
  every embedded contract and baseline and `--check` failing CI on any staleness. The five
  `detect.dirs` divergences became a reasoned overlay, `agency-categories.json` was deleted
  in favour of the catalog's own `divisions.json`, seeding is recursive, and a submodule-less
  clone fails with the init command instead of a Rust backtrace. See
  [261103_pinned-catalog-submodules.md](./261103_pinned-catalog-submodules.md).
- Shipped the Rules integration (issue #29, M1; PR #61): native Rust compilation of the
  four `agency-rules` renderers with 32/32 byte-parity against a fresh upstream compile
  and an 88/88 adversarial matrix; a second managed corpus with its own baseline,
  refresh and index; ledger tracking via a `domain` discriminator filtered out of six
  agent surfaces; OpenCode plugin registration; a Rules workspace in all 12 locales.
  Also fixed two pre-existing data-integrity defects (`atomic_write` never flushed, so
  a full disk silently truncated the ledger; `load_ledger` treated any read error as an
  empty ledger). See
  [261103_rules-integration.md](./261103_rules-integration.md).
- Fixed #58 (Runbooks never populated by app-provisioned catalogs): snapshot
  provisioning now discovers divisions from `divisions.json` instead of the
  removed `scripts/convert.sh`, and extracts `strategy/`, `divisions.json` and
  `scripts/` verbatim instead of an allow-list of division dirs. `provision_managed`
  repairs a stale division-only root, and a broken manifest now surfaces an error
  instead of a misleading "sync to unlock". Real-tarball probe: 18 divisions,
  298 agents, 17 runbooks. See
  [261102_runbooks-snapshot-provisioning.md](./261102_runbooks-snapshot-provisioning.md).
- Fixed the in-app updater and closed #23/#24 (PR #38): the release pipeline now
  publishes signed `latest.json` manifests, Linux/Windows updater artifacts are signed,
  the Linux tarball is regenerated after the Wayland repack, stable/beta channels are
  persisted and selectable, and the auto-install consent keys exist in all 11
  translations. Added `mise run verify-updater` and `mise run set-updater-secrets`.
  See [261102_updater-pipeline.md](./261102_updater-pipeline.md).
- Added a Draft 2020-12 JSON Schema for the tool registry (issue #30): `src-tauri/data/_schema.json`
  validates the bundled `tools.json` in the IDE (VS Code mapping) and in `cargo test --test tools_schema`,
  which also asserts the schema still rejects malformed entries. See
  [261101_tools-json-schema.md](./261101_tools-json-schema.md).
- Verified Antigravity role-flag defaults at runtime (issue #27): no change needed; omitted
  `mainAgent` defaults to `false` (protobuf `omitempty`), unknown keys are tolerated, the CLI
  ignores flags for `--agent` selection. Found a stale foreign artifact (`mainAgents: true`)
  that will be replaced by reconcile. See
  [261101_antigravity-role-flag-runtime.md](./261101_antigravity-role-flag-runtime.md).
- Declared per-role Antigravity tool lists (issue #26): `agents-orchestrator` gets the 11-tool
  orchestration surface, specialists get the 7 execution tools. See
  [261101_antigravity-tool-lists.md](./261101_antigravity-tool-lists.md).
