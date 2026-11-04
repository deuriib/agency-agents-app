# November 2026 — Antigravity parity + updater pipeline

## Current State
**v0.12.0** (`5234cca`, PR #95) is the latest tag. PR #102 (merged `3ba967d`) landed right
after it, so the updater fixes it carries ship in **0.13.0**. `main` is green. Every
release since 0.8.0 is still a **draft with Linux artifacts only**: `latest.json`, the
Windows payloads and the notarized macOS build are missing, so the updater cannot serve
them yet.

## Goals
- [ ] Publish: signing secrets, local macOS release run, `verify-updater`.
- [ ] Native review of the 12-locale strings (Rules, plus the three `updates.*` keys from
  #102).
- [ ] Decide #56 option B (per-installer manifests + signing for `.deb` / `.rpm`) or accept
  download-only Linux packages permanently.
- [ ] Remaining #61 follow-ups: #64, #65, #70, #71, #76, #98. #62, #63, #66, #68, #69 and
  #72 have shipped.

## Recent Achievements
- Made the updater stop promising what it cannot deliver (#56, #57, #47). A `.deb` / `.rpm`
  install no longer offers an in-place update that could never succeed — the plugin picks
  its installer from the bundle type baked into the binary, while a manifest carries one
  artifact per platform and ours is the AppImage — so the build is told the truth, the
  release's own installer is offered as a download, and the gate refuses *before* the
  ~85 MB. `inAppInstall` joined `installable` in the payload because the first is a
  property of the build and the second of the release; collapsing them is what produced
  the bug. `InvalidUpdaterFormat` and `SelfUpdateUnsupported` are their own error variants,
  so a local dead end is never reported as a network failure again. Release notes are now
  flattened to plain text **before** truncation (#57), which also makes a half-cut URL
  unrepresentable. The retired `agencyagents.app` host left the CSP, the network panel and
  all 12 locales (#47). 596 → 626 Rust tests, 10/10 CI checks, zero new dependencies. See
  [261104_updater-package-installs.md](./261104_updater-package-installs.md).
- Stopped re-running the whole PR graph on every merge (issue #90), in the order the
  issue demanded: gate first, remove second. `main: PR gate` (`26500514`) requires the
  eight PR Check jobs plus `gitleaks`, each pinned to the GitHub Actions integration so a
  check run cannot be spoofed, with `strict_required_status_checks_policy` so a stale PR
  cannot merge, and it blocks force-pushes and deletion. `allow_update_branch` was
  switched on in the same breath, because requiring an up-to-date branch without the
  button is how a gate gets disabled in a month. With that closed, `rust-lint`,
  `rust-test` and `frontend` became `pull_request`-only and the four manifest jobs stayed
  on `push: main` as a 39s canary: **252s → 39s of job time per merge (−85%)**. Proven,
  not assumed — probe PR #100 (`cargo fmt` red, merge refused with "the base branch policy
  prohibits the merge") and a sacrificial ruleset on a throwaway ref (`GH013: Required
  status check "Rust tests" is expected` for a direct push). The docs also state what the
  gate is *not*: no `bypass_actors`, but an admin can still edit the ruleset, so skipping
  it is deliberate and visible rather than impossible. See
  [261104_drop-push-main-graph.md](./261104_drop-push-main-graph.md).
- Gave the release automation its own identity (issue #91): both `release-please.yml` and
  `release-lock.yml` mint a GitHub App installation token and push with it, because a
  `GITHUB_TOKEN` push starts no workflow run at all — which is why `release-lock.yml` never
  fired and the lock stayed one release behind. No fallback credential, so a missing App key
  fails the job instead of passing silently; credentials and key rotation live in
  `docs/BUILD.md#release-automation-token`, and `scripts/release-workflows.test.mjs` pins the
  wiring in both workflows with 13 test cases. Supersedes the "acceptable because the checks need a maintainer"
  trade-off recorded for #81; AC2–AC5 stay unproven until the App exists and a release runs.
  See [261103_release-please-app-token.md](./261103_release-please-app-token.md).
- Detected the Antigravity app (issue #69): `.gemini/antigravity` joined the CLI and IDE roots
  in the tools overlay, so a desktop-app-only user is no longer invisible. Three tests on the
  real detection path; `agentsDir` deliberately unchanged. See
  [261103_antigravity-app-detection.md](./261103_antigravity-app-detection.md).
- Made `src-tauri/Cargo.lock` track the release version (issue #81): a
  `cargo metadata --locked` gate in PR Check and `mise run check-lock`, plus
  `release-lock.yml` committing the sync into the release PR so the tagged commit
  matches its manifest. See [261103_cargo-lock-sync.md](./261103_cargo-lock-sync.md).
- Lifted the multi-line install guard (issue #63): the literal block, all three chomping
  indicators, the indent indicator and the quoted fallbacks are ported from `yaml@2` and
  proven against a generated 28-case oracle; only the folded (`>`) shape stays refused, with
  a message that names it. See [261103_block-scalar-folding.md](./261103_block-scalar-folding.md).
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
