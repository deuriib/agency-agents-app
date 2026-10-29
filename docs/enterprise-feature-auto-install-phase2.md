# Phase 2 — Foundation: Opt-in Automatic Install

**Runbook**: Enterprise Feature Development (NEXUS-Sprint) | **Feature**: Wire the inert "Install updates automatically" toggle to a real off-by-default background download → verify → install flow
**Branch**: `feature/auto-install-toggle` | **Date**: 2026-10-28 | **Status**: Phase 2 complete — awaiting Foundation gate

## 1. What shipped (foundation only)

| Area | Change | File |
|---|---|---|
| Settings schema | New `update_auto_install: bool`, `#[serde(default)]`, default `false` | `src-tauri/src/commands/settings.rs:117` |
| Settings default | `update_auto_install: false` in `Settings::default()` | `src-tauri/src/commands/settings.rs:183` |
| Frontend type | New `updateAutoInstall: boolean` + `SETTINGS_DEFAULTS` entry | `src/lib/types.ts:48`, `src/lib/types.ts:81` |
| Toggle | Inert `disabled` row replaced by a live toggle bound to `settings.effective.updateAutoInstall` | `src/lib/components/SettingsSectionUpdates.svelte` |
| Consent | First-time enable opens the existing `DestructiveConfirm` modal (`confirmVariant="primary"`) — no new component | `src/lib/components/SettingsSectionUpdates.svelte` |
| Copy | Live hint + consent title/body/confirm strings (English; other locales fall back to English) | `src/lib/i18n/locales/en.ts` |
| Tests | Defaults + camelCase round-trip pinned | `src-tauri/src/commands/settings.rs` |

## 2. Reuse analysis (Four Sacred Rules)

- **Extended** the existing `Settings` struct rather than adding a parallel settings file — same `settings.json`, same atomic-write path, same corrupt-file fail-closed semantics.
- **Extended** `SettingsSectionUpdates.svelte` rather than creating a new settings section.
- **Reused** `DestructiveConfirm.svelte` for the consent dialog rather than creating a one-off modal. Its `confirmVariant="primary"` already supports additive confirms.
- **No new files** in this phase. The only new file is the Phase 1 planning doc (documentation, not code).

## 3. Scope boundary (what Phase 2 deliberately does NOT do)

Per the runbook's foundation/build split, the scheduler and background install behavior are Phase 3:

- No scheduler change yet — `spawn_auto_check_scheduler` still only checks.
- No `autoInstalling` store state yet.
- No journal events yet.
- The toggle persists the preference but has no runtime effect until Phase 3 wires the scheduler.

This is intentional and safe: the setting is off by default, and even when on it currently changes nothing at runtime.

## 4. Evidence (verified, not inferred)

### Frontend

```
npm run check
svelte-check found 0 errors and 0 warnings
```

### Backend

```
cargo check --lib
EXIT=0
```

### Settings tests

```
cargo test --lib commands::settings::tests
running 26 tests
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 282 filtered out
EXIT=0
```

### Full backend lib suite (regression check)

```
cargo test --lib
test result: ok. 307 passed; 0 failed; 1 ignored
```

The 6 failures observed during the first Phase 2 run were **fixed**, not waived (DENY-04):

| Failure | Root cause | Fix |
|---|---|---|
| `registry_loads_and_derives_installable` | Test expected 17 tools; catalog now has 19 (`piAgent`, `ohMyPi`). Backend `IMPLEMENTED_FORMATS` also omitted `antigravity-md`, which `render()` supports. | Count → 19; added `antigravity-md` to `IMPLEMENTED_FORMATS`; ids corrected to camelCase. |
| `render::tests::pi_agent_native_agents_shape_and_dests` | Test used kebab id `pi-agent` (registry keys on camelCase `piAgent`); project dest assertion said `.agents/agents` while `tools.json` says `.pi/agents`. | Id → `piAgent`; dest assertion → `.pi/agents`. |
| `render::tests::oh_my_pi_native_agents_shape_and_dests` | Test used kebab id `oh-my-pi`. | Id → `ohMyPi`. |
| `install::tests::track_writes_no_file` | Compared `record.dest` (JSON template preserves `/`) to a `PathBuf` string (`\` on Windows). | Compare `PathBuf` to `PathBuf`. |
| `install::tests::tracked_conversion_slug_update_reuses_existing_destination` | Same path-separator mismatch. | Compare `PathBuf` to `PathBuf`. |
| `install::tests::install_writes_then_reconciles_through_states` | Real bug: `write_agent_files_to` hashed a composite frame even for single-file renders, so `sha256(on_disk) != rendered_hash` and every single-file install reconciled as `Modified`. | Single-file renders now hash the file bytes directly; composite hash is reserved for true multi-file renders. |

**Remaining known drift (not a failing test, recorded for Phase 3):** the frontend `toolRegistry.ts` marks `kimi-agent`, `openclaw-workspace`, and `vibe-toml` installable, but the backend `IMPLEMENTED_FORMATS` still omits them; `track_agent_record` also uses single-file `render()` and would fail for those formats. Owner: Backend Architect.

### Baseline evidence (inert state)

The pre-change inert row is removed in the diff (`git diff` shows the `disabled` input + `is-disabled` styling deleted). No native screenshot is possible in this environment (the Tauri window cannot be auto-driven); the diff is the baseline evidence, and Phase 3 will capture the live toggle in the `?shim=1` browser harness if needed.

## 5. Foundation Gate (Evidence Collector)

| Criterion | Status |
|---|---|
| Feature branch exists | ✅ `feature/auto-install-toggle` |
| Setting persists with backward compat | ✅ `#[serde(default)]`, default false, 26/26 tests |
| Frontend type mirrors backend wire shape | ✅ camelCase round-trip test |
| Toggle is live (no longer inert) | ✅ diff removes `disabled` + `is-disabled` |
| Consent is explicit on first enable | ✅ reused `DestructiveConfirm` |
| Offline Mode gate untouched | ✅ no change to `require_network` / `paranoidMode` |
| No runtime behavior change yet | ✅ scheduler untouched |
| Frontend green | ✅ svelte-check 0/0 |
| Backend compiles | ✅ cargo check exit 0 |
| No new code files | ✅ reuse analysis above |

**Verdict**: ✅ PASS — proceed to Phase 3 (Build) pending user approval.

## 6. Next actions (Phase 3 — Build)

1. Extend the scheduler: after a successful `run_check`, if `update_auto_install` is on AND not Offline Mode AND not skipped AND the release is installable → run `run_install`.
2. Add `autoInstalling` state + `install(version, { auto })` to `UpdaterStore`; abort on toggle-off.
3. Honor the skip-list and never auto-install a skipped version.
4. Emit journal events (started / verified / installed / failed / canceled).
5. Keep relaunch user-gated (`update_relaunch` stays manual).
6. Add unit tests for the scheduler gates + store auto path; integration test for the offline gate and cancel.

Awaiting approval to enter Phase 3.