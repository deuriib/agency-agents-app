# 261103_frontend-test-runner

## Objective

Issue #62. Give the frontend a regression net. The Rules workspace (#61) shipped
~1,650 lines of Svelte 5 + TypeScript with **zero** automated tests, and a
critical defect got through every gate: with **Project** scope selected and no
folder picked, the install-state derivation collapsed the project path to `null`
— the user ledger key — so the UI rendered "Installed" with the user-global path
and an enabled **Uninstall** that deleted the user's global rule.

`svelte-check` was green and the build was green. It was a logic bug, not a type
error — exactly what a unit test catches and a type-checker cannot.

## Outcome

- ✅ **Vitest** (`npm run test`) on jsdom with `@testing-library/svelte`, wired
  into `mise run ci` (`test-ui`), `mise run test`, and the `frontend` PR job.
- ✅ Pure semantics extracted to `src/lib/rules/derive.ts` — `effectiveRoot`,
  `needsProjectRoot`, `installFor`, `installsFor`, `installedHarnesses`,
  `filterRules`, `ruleCategories`, `invalidCount`, `previewEntry`,
  `installBlockReason`, `ruleListState`. The store and both components delegate;
  no behaviour moved outside the pure functions' reach.
- ✅ 48 tests in 4 files, **9.5 s** (TST-05 budget is 30 s):
  - `derive.test.ts` (18) — install identity and precedence
  - `stores/rules.test.ts` (10) — IPC boundary, stale-response guards, cache
  - `components/RuleInstallControls.test.ts` (12) — what the pane renders
  - `components/Rules.test.ts` (8) — one test per list state + per-rule state
- ✅ **Revert-proof** (each fix reverted locally, suite went red, restored):
  | Reverted | Failing test |
  | --- | --- |
  | pre-fix ledger matcher (no `scope` in identity, no no-root guard) | `installFor › never answers a project-scope question with the user row` |
  | `uninstall`'s no-root guard | `project scope without a folder › uninstall makes zero IPC calls` |
  | `load`'s fail-closed clear | `load › clears the install truth when the read fails` |
  | `installBlockReason` precedence order | `installBlockReason › orders the reasons most-specific-first` |

## Files

**New**

- `vitest.config.ts`, `src/test/setup.ts`
- `src/lib/rules/derive.ts` + `derive.test.ts`
- `src/lib/stores/rules.test.ts`
- `src/lib/components/RuleInstallControls.test.ts`, `Rules.test.ts`
- `src/lib/test/fixtures.ts`

**Modified**

- `src/lib/stores/rules.svelte.ts` — delegates the pure derivations
- `src/lib/components/RuleInstallControls.svelte` — uses `installBlockReason`
- `src/lib/components/Rules.svelte` — renders from `ruleListState`
- `package.json` (+4 dev deps: vitest 5.1.0, `@testing-library/svelte` 5.4.2,
  `@testing-library/jest-dom` 6.9.1, jsdom 30.1.3), `.mise.toml`,
  `.github/workflows/pr-check.yml`, `CONTRIBUTING.md`

## Patterns Applied

- **Derive, don't copy.** Reasoning the UI needs is a pure function, so a test
  reaches it without rendering. Components are then thin.
- **One stub at the boundary.** `vi.mock("@tauri-apps/api/core")` is the only
  fake; nothing mocks the module under test, so the tests would catch a regression
  in the store itself.
- **Reasons are keys, not sentences.** `installBlockReason` returns
  `"invalid" | "noProjectRoot" | …`; the component maps them through i18n, so
  precedence is testable in every locale.

## Known gaps / follow-ups

- **No coverage gate yet.** `@vitest/coverage-v8` and the TST-03 floors
  (line ≥80 / branch ≥75 / function ≥85) are a separate change: enabling a floor
  today would fail on untouched components.
- `AgentsWorkspace.svelte`, `Runbooks.svelte`, `Teams.svelte` and the stores for
  agents/teams/projects are still untested; they can now be covered with the
  same runner without new infrastructure.
- The detail pane's scope picker (`RuleDetail.svelte`) is covered indirectly
  through `RuleInstallControls`; a component test for the folder-picker flow
  needs `@tauri-apps/plugin-dialog` stubbed and is the natural next slice.
