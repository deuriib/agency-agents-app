# Phase 1 — Requirements & Architecture: Opt-in Automatic Install

**Runbook**: Enterprise Feature Development (NEXUS-Sprint) | **Feature**: Wire the inert "Install updates automatically" toggle to a real off-by-default background download → verify → install flow (relaunch prompt only)
**Date**: 2026-10-28 | **Status**: Phase 1 complete — awaiting Architecture Review gate

## 1. Feature scope (single sentence)

Turn the currently inert "Install updates automatically" toggle into a real, off-by-default opt-in flow that, when enabled, downloads, verifies, and installs an approved update in the background and prompts the user only to relaunch — without weakening the existing manual check/install path, the Offline Mode gate, or the skip-list.

## 2. Stakeholder alignment (Project Shepherd)

| Stakeholder | Interest | Cadence | Owner |
|---|---|---|---|
| End users (all platforms) | Hands-off updates; fewer clicks | Weekly status | Project Shepherd |
| Maintainer (Michael) | Release quality; no surprise installs | Bi-weekly | Executive Summary Generator |
| Compliance team | No auto-exec of unverified payloads; consent; auditability | Monthly | Legal Compliance Checker |
| Security | Signature verification before install; no downgrade; no silent relaunch | Monthly | Legal Compliance Checker |
| Finance | No new infra; reuse existing updater endpoint | Monthly | Finance Tracker |
| Engineering | Backward-compat settings; offline gate preserved | Daily | Agents Orchestrator |

## 3. User research (UX Researcher)

- Existing manual flow is one-click download → verify → install → relaunch, surfaced via title-bar pill + Settings → Network → Updates. Users already understand the steps; auto-install removes friction only after explicit opt-in.
- Consent must be explicit and reversible: toggle defaults off; enabling requires the user to have previously seen at least one successful manual check (so they understand what an "update" is). Disabling mid-flight must cancel the in-flight download cleanly.
- Relaunch is never forced silently: install completes in the background, then a single non-blocking "Relaunch now" prompt is shown; app keeps running on the old binary until the user accepts.
- Offline Mode (`paranoidMode`) remains a hard gate: auto-check and auto-install are both suspended while Offline Mode is on, and resume (or re-evaluate) when it turns off.
- Failure UX mirrors manual: inline error + "Try again" in Settings; title-bar pill stays visible so the user can fall back to manual install.

## 4. Compliance requirements (Legal Compliance Checker)

See `memory-bank/scans/auto-install-compliance.md` (to be created in Phase 2). P0 gates:
- Opt-in only: default `false`; no background activity unless the toggle is on AND Offline Mode is off.
- Consent & transparency: toggle label + hint already shipped in 18 locales; add a one-time confirmation dialog the first time auto-install is enabled, stating that approved updates will install without further prompts.
- Verification before install: reuse the existing signed-manifest + minisign signature verification; auto-install must refuse to proceed if verification fails (no fallback to unverified).
- No downgrade / no silent channel switch: only install versions newer than current per the manifest; channel stays "Stable".
- Cancellation safety: disabling the toggle mid-download aborts the download via the same abort path as unmount; partial files are removed; ledger/settings remain consistent.
- Auditability: log auto-install events (started, verified, installed, failed, canceled) to the Activity Journal with version + timestamp; no PII.
- No new secrets: reuse existing updater key; no credentials in the bundle (DENY-01).
- Draft pending human review (DENY-07).

## 5. UX foundation (UX Architect / UI Designer)

- Settings → Network → Updates "Row 2b" becomes a live toggle bound to a new `updateAutoInstall` setting, mirroring `updateAutoCheck` (toggleable even in Offline Mode so the preference is configurable; behavior gated at runtime).
- Remove the `is-disabled` styling and the "Available once the update channel is live" hint; replace with a live hint describing background download → verify → install → relaunch prompt.
- Title-bar pill: when an auto-install is in progress, show a small spinner in place of the × (already supported by `installing` flag); when auto-install completes, show "Relaunch now" affordance.
- New state: `autoInstalling` (distinct from manual `installing`) so the UI can distinguish a user-initiated install from a background one; both reuse the same progress indicator.
- Confirmation dialog component (new, small) for first-time enable; reusable pattern for future consent gates.
- Brand: no new brand assets; reuse existing toggle + spinner.

## 6. System architecture (Backend Architect)

Recommended design (extends existing updater, no rewrite):
- **Setting**: add `updateAutoInstall: bool` (serde-defaulted `false`) to the settings schema, persisted in `settings.json` alongside `updateAutoCheck` and `skippedUpdateVersions`. Mirror on frontend `Settings` type + defaults.
- **Auto-check scheduler**: the existing auto-check is triggered by app launch + a daily timer when `updateAutoCheck` is on. Extend the scheduler: when an update is found AND `updateAutoInstall` is on AND not Offline Mode AND the version is not skipped AND not already installed → enqueue an auto-install job.
- **Auto-install job**: reuse `update_install(version)` backend command end-to-end (download → verify signature → install). Add an `auto: bool` flag so the backend can (a) skip re-prompting and (b) emit a distinct event for the journal. The frontend's `UpdaterStore.install()` already drives download/verify/install; call it with `auto=true` semantics handled in store.
- **Concurrency / backpressure (ENG-11)**: at most one install in flight; if a manual install is requested while auto-install is running, the manual request wins (cancel auto, surface manual progress); if auto-install is running and the user disables the toggle, abort via the existing cancel token.
- **Offline gate**: scheduler checks `paranoidMode` before every step; if it flips on mid-download, abort and leave `available` intact so manual install remains possible.
- **Relaunch is user-gated**: auto-install never calls `update_relaunch`; it sets `installComplete = true` and lets the UI offer "Relaunch now". The backend's restart scheduling stays manual-only.
- **Skip-list honored**: an auto-install never overrides a skipped version; the user must explicitly un-skip (via the × path) before auto-install can proceed.
- **Idempotency**: re-running the scheduler with the same available version is a no-op if an install is already complete/queued.

Backend touchpoints: `commands/settings.rs` (new key), `commands/updater.rs` (auto flag + event), scheduler in `lib.rs`/updater module. Frontend: `stores/settings.svelte.ts`, `stores/updater.svelte.ts`, `SettingsSectionUpdates.svelte`, `UpdateIndicator.svelte`, i18n hint.

## 7. RICE backlog (Sprint Prioritizer)

7 sub-tasks, 1-sprint plan (~6 pw of 6 pw capacity, tight):

| # | Task | Effort (pw) | Owner |
|---|---|---|---|
| 1 | Settings schema: `updateAutoInstall` key (Rust serde-default + frontend type/default) | 0.5 | Backend Architect |
| 2 | Scheduler extension: auto-check → auto-install enqueue with gates (offline, skip, version, in-flight) | 1.5 | Backend Architect |
| 3 | Updater store: `autoInstalling` state + `install(version, {auto})` + cancel on toggle-off | 1.0 | Frontend Developer |
| 4 | Settings UI: live toggle (remove inert), first-time consent dialog | 1.0 | UI Designer + Frontend Developer |
| 5 | Title-bar pill: spinner during auto-install, "Relaunch now" on complete | 0.5 | Frontend Developer |
| 6 | Activity Journal: auto-install events (started/verified/installed/failed/canceled) | 0.5 | Frontend Developer |
| 7 | Tests: unit (scheduler gates, store auto path) + integration (offline gate, cancel, skip honor) | 1.0 | Test Results Analyzer |

MoSCoW: Must = 1,2,3,4,6,7; Should = 5 (nice-to-have, can ship with manual fallback). Total 6.0 pw = 100% of sprint capacity → risk: no buffer; recommend dropping Should if slipping.

## 8. Brand impact (Brand Guardian)

No new brand surfaces. Reuse existing toggle, spinner, pill. Consent dialog follows existing modal styling. No brand voice changes; hint copy already translated in 18 locales (only the hint text changes from "available once channel is live" to the live description).

## 9. Budget framework (Finance Tracker)

- 1 sprint × 6 pw = 6 pw capacity; plan uses 6.0 pw (0% buffer) — flag for scope risk.
- No new infrastructure: reuses existing updater endpoint + signed manifest.
- Risk: consent-dialog + cancel plumbing may overrun; stop condition: ship Must items 1-4,6,7 and defer 5 to next sprint if needed.

## 10. Architecture Review Gate (Project Shepherd + Reality Checker)

| Criterion | Status |
|---|---|
| Reuse over creation (extend updater, not rewrite) | ✅ Extends existing updater command + store |
| No new files without justification | ✅ New consent dialog component justified (no existing reusable modal with consent semantics); new settings key justified |
| Backward compat (`updateAutoInstall` serde-defaulted false; old settings load) | ✅ |
| Compliance P0 gates addressed | ✅ (pending human review) |
| Offline gate preserved | ✅ |
| Skip-list honored | ✅ |
| No silent relaunch | ✅ (relaunch stays user-gated) |
| Test plan defined | ✅ (unit + integration) |
| Budget within limits | ⚠️ 0% buffer — scope risk flagged |

**Verdict**: ✅ PASS — proceed to Phase 2 (Foundation) pending user approval, with the noted 0%-buffer scope risk.

## Next actions (Phase 2)
1. DevOps: feature branch `feature/auto-install-toggle` + CI gate.
2. Backend: `updateAutoInstall` setting key + serde default + frontend type/default.
3. Frontend: live toggle wiring (remove inert) + consent dialog scaffold.
4. Evidence Collector: baseline screenshots of inert state.

Awaiting approval to enter Phase 2.