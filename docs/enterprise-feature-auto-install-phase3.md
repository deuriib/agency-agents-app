# Phase 3 — Build: Opt-in Automatic Install

**Runbook**: Enterprise Feature Development (NEXUS-Sprint) | **Feature**: Wire the inert "Install updates automatically" toggle to a real off-by-default background download → verify → install flow (relaunch prompt only)
**Branch**: `feature/auto-install-toggle` | **Date**: 2026-10-28 | **Status**: Phase 3 complete — awaiting Build gate

## 1. What shipped

### Backend (Rust)

| Area | Change | File |
|---|---|---|
| Error model | `AppError::InstallCanceled` → wire code `install_canceled` | `src-tauri/src/error.rs` |
| Cancellation | `CancelToken(Arc<AtomicBool>)` — `request` / `reset` / `is_requested`, shared by scheduler, settings, and plugin backend | `src-tauri/src/commands/updater.rs:166` |
| State | `UpdaterState.auto_install` (lifecycle + single-flight guard) and `.auto_install_cancel` | `src-tauri/src/commands/updater.rs:194`, `:213` |
| Plugin split | `Update::download` (fetches **and** verifies minisign) is now separate from `Update::install` (bundle swap) | `src-tauri/src/commands/updater.rs:376` |
| Events | `update:auto-install` with `AutoInstallEvent { phase, version, message? }` | `src-tauri/src/commands/updater.rs:640`, `:651` |
| Gates | `auto_install_blocker` → `Disabled` / `Offline` / `SkippedVersion` / `AlreadyRunning` | `src-tauri/src/commands/updater.rs:694`, `:724` |
| Runner | `run_auto_install` maps gate refusals to skips, cancel to `Canceled`, real failures to `Failed` | `src-tauri/src/commands/updater.rs:767` |
| Consent hook | `handle_auto_install_consent_change` aborts an in-flight install on toggle-off **or** Offline Mode engaging | `src-tauri/src/commands/updater.rs:853` |
| IPC | `update_auto_install_cancel` (no `require_network` — must work while going offline) | `src-tauri/src/commands/updater.rs:1060` |
| Scheduler | `read_scheduler_inputs` gates discovery on `update_auto_check \|\| update_auto_install`; `maybe_auto_install` runs after a successful check | `src-tauri/src/commands/updater.rs:1213`, `:1244` |
| Wiring | Command registered; `settings_set` / `settings_reset` snapshot consent before writing and act on the transition | `src-tauri/src/lib.rs`, `src-tauri/src/commands/settings.rs` |

### Frontend (Svelte 5)

| Area | Change | File |
|---|---|---|
| Types | `install_canceled` in `AppErrorPayload` + `appErrorMessage`; `AutoInstallPhase`, `AutoInstallEvent` | `src/lib/types.ts` |
| API | `updateAutoInstallCancel()` | `src/lib/api.ts` |
| Store | `autoInstalling` / `autoInstallVersion` / `autoInstallPhase`, `attachAutoInstallListener` / `detachAutoInstallListener`, `handleAutoInstallEvent`, `cancelAutoInstall`; manual `install()` refuses while a background install is in flight | `src/lib/stores/updater.svelte.ts:96`, `:198`, `:233`, `:281` |
| Root wiring | Listener attached once in `onMount`, detached in cleanup | `src/routes/+layout.svelte` |
| Settings card | Progress row + Cancel for background installs; standalone progress / relaunch / failure blocks that survive `available === null` | `src/lib/components/SettingsSectionUpdates.svelte:327`, `:351` |
| Title-bar pill | Spinner on `installing \|\| autoInstalling` | `src/lib/components/UpdateIndicator.svelte` |
| Journal | New `appUpdate` action (icon, verb, row text) + `updates.journalInstalled` | `src/lib/stores/activity.svelte.ts`, `src/lib/components/ActivityHistory.svelte` |
| Copy | 6 new `updates.*` keys + `activity.action.appUpdate` (other locales fall back to English) | `src/lib/i18n/locales/en.ts` |

## 2. Deviations from the Phase 1 architecture

Four changes to the approved design. Each is a correctness fix, not a preference.

| Phase 1 sketch | Shipped | Why |
|---|---|---|
| `updater.install(version, { auto })` driven from the renderer | Install driven by the **Rust scheduler**; the store only observes events | A renderer-driven install dies with the view. Navigating away from Settings (or never opening it) would silently abandon a download the user opted into. The backend owns the work, the renderer is a spectator. |
| "Manual wins" when both paths target the same version | Manual `install()` is **refused** while a background install is in flight | Two concurrent installs race the bundle swap for the *same* version. The UI never offers the button during an auto-install, so the guard only catches a stale click. |
| Single `install()` call, cancel before it starts | `download` (verify) and `install` (swap) split, cancel **between** them | Gives cancel a precise, safe checkpoint: after authenticity is proven, before anything on disk changes. A cancel therefore leaves no partial artifact and no swapped bundle, and the cached "available" entry stays valid for a manual retry. |
| Separate audit journal for updates | `appUpdate` action added to the **existing** Activity Journal | One history surface beats two. Satisfies the Phase 1 auditability P0 (installed + failed are recorded with version) without a second UI. |
| — (not in Phase 1) | Offline Mode engaging also requests a cancel | Consistency with consent revocation. Honest scope: the cancel point is post-download, so it stops the **install**, not bytes already in flight — nothing on disk changes once the flag is set. |

## 3. Gate order (the security argument)

`auto_install_blocker` is evaluated before any work, in this order:

1. **Offline Mode / corrupt settings** → `Offline`. Corrupt settings fail closed exactly like `require_network`.
2. **`update_auto_install` off** (including first launch) → `Disabled`.
3. **Version on the skip-list** → `SkippedVersion`. A dismissed release is never installed unattended.
4. **A background install already running** → `AlreadyRunning` (single-flight).

Then `run_auto_install` delegates to the existing `run_install`, which re-applies defense in depth: `require_network("update_check")`, cached-version match, installability, explicit downgrade rejection.

Offline Mode is checked first on purpose: when both conditions hold, "offline" is the reason the user must act on.

The relaunch is never automatic. `update_relaunch` stays user-gated, so an unattended install can never restart the app under the user's feet.

## 4. Evidence (verified, not inferred)

### Backend suite

```
cargo test --lib
test result: ok. 327 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
EXIT=0
```

307 → 327 = **20 new tests** (the ignored test is the pre-existing upstream parity test that requires `AGENCY_AGENTS_PARITY_ROOT`).

| Test | Pins |
|---|---|
| `cancel_token_round_trips_request_and_reset` | reset re-arms the next attempt |
| `cancel_token_clones_share_the_same_flag` | scheduler and backend observe the same flag |
| `auto_install_blocker_reports_disabled_by_default` | off by default |
| `auto_install_blocker_reports_disabled_on_first_launch` | no settings file ⇒ not opted in |
| `auto_install_blocker_reports_offline_on_corrupt_settings` | fail closed |
| `auto_install_blocker_reports_offline_when_paranoid_mode_is_on` | Offline Mode is a hard gate |
| `auto_install_blocker_reports_skipped_version` | skip-list honored, other versions unaffected |
| `auto_install_blocker_clears_when_all_gates_pass` | no false positives |
| `auto_install_blocker_reports_already_running` | single-flight |
| `auto_install_skips_disabled_without_calling_backend` | backend never reached when off |
| `auto_install_skips_offline_without_calling_backend` | backend never reached when offline |
| `auto_install_installs_when_all_gates_pass` | happy path clears the cached entry |
| `auto_install_maps_cancel_and_preserves_cache` | cancel keeps the manual path open |
| `auto_install_maps_gate_refusal_to_skip` | a refusal is not an error, guard is released |
| `auto_install_records_failure_and_returns_error` | typed failure recorded for the card |
| `auto_install_consent_change_requests_cancel_only_on_revocation` | toggle-off and offline-engage cancel; other transitions inert |
| `auto_install_event_wire_shape` | `phase` / `version` / optional `message` |
| `auto_install_phases_serialize_as_expected_tokens` | `started`/`verified`/`installed`/`failed`/`canceled` |
| `auto_install_status_serializes_with_phase_tag` | status tag matches event tokens |
| `install_canceled_serializes_to_install_canceled_code` | wire code `install_canceled` |

### Compilation and lint

```
cargo check --lib   → EXIT=0, 1 pre-existing warning (`unused variable: app` in `update_check_now`)
cargo test --lib --no-run → 7 pre-existing warnings, 0 new
```

### Frontend

```
npm run check   → svelte-check found 0 errors and 0 warnings
npm run build   → ✓ built in 19.90s, EXIT=0
```

## 5. Known gaps (stated, not hidden)

1. **No frontend test runner exists in this repo.** The event → UI mapping (`handleAutoInstallEvent`) is exercised only by `svelte-check` and the production build, not by assertions. Adding Vitest is a separate change; the mapping is a pure switch over a 5-value union, and the wire shape it consumes is pinned on the backend.
2. **No live end-to-end run.** The Tauri window cannot be auto-driven here, and a real background install needs a signed release newer than the running build. Backend behaviour is covered by 20 unit tests through the `InstallBackend` trait; the plugin call itself (`Update::download` → `Update::install`) is untested by design — it is the vendor's code path, reached only with a real artifact.
3. **Pre-existing tool-registry drift is untouched** (`kimi-agent`, `openclaw-workspace`, `vibe-toml` marked installable in `toolRegistry.ts`, absent from `IMPLEMENTED_FORMATS`). Out of scope for this feature; still open from Phase 2.
4. **Cancellation granularity.** The flag is polled at one checkpoint (post-verify, pre-swap). A download that has not reached verification when the user cancels will finish downloading and then discard the bytes. Stopping mid-transfer would require plugin-level abort support that `tauri-plugin-updater` 2.10.1 does not expose.

## 6. Build Gate (Evidence Collector)

| Criterion | Status |
|---|---|
| Off by default, two independent opt-ins | ✅ `update_auto_install` default false + `paranoid_mode` hard gate |
| Offline Mode remains a hard gate | ✅ blocker reports `Offline` first; corrupt settings fail closed; engaging it cancels in flight |
| Never silent relaunch | ✅ `update_relaunch` untouched, still user-gated |
| Reuses existing signature verification | ✅ `Update::download` is the plugin's own sha256 + minisign path |
| Skip-list honored | ✅ `SkippedVersion` gate + `skipped` check in `maybe_auto_install` |
| No new secrets / endpoints | ✅ none added |
| Consent revocation aborts install | ✅ `handle_auto_install_consent_change`, 1 test |
| Single-flight | ✅ `AlreadyRunning`, 1 test |
| Auditability (Phase 1 P0) | ✅ tracing log per phase + `appUpdate` journal rows (installed / failed) |
| Backend suite green | ✅ 327 passed, 0 failed, 1 ignored (pre-existing) |
| No new warnings | ✅ 7 pre-existing test-build, 1 pre-existing lib warning |
| Frontend green | ✅ svelte-check 0/0, production build OK |
| No new files in this phase | ✅ all changes extend existing modules (only this doc is new) |

**Verdict**: ✅ PASS — proceed to Phase 4 (Verify) pending user approval.

## 7. Next actions (Phase 4 — Verify)

1. Independent verification pass: re-run both suites from a clean state, confirm the diff matches this document.
2. Manual smoke test on a signed build where feasible: enable the toggle, force an auto-check, watch `started → verified → installed`, confirm the relaunch prompt and the journal row.
3. Confirm the cancel path leaves the app on the old version with the manual install still offered.
4. Confirm Offline Mode suppresses the whole path even with the toggle on.
