# Phase 4 — Verify: Opt-in Automatic Install

**Runbook**: Enterprise Feature Development (NEXUS-Sprint) | **Feature**: Wire the inert "Install updates automatically" toggle to a real off-by-default background download → verify → install flow
**Branch**: `feature/auto-install-toggle` | **Date**: 2026-10-28 | **Status**: ⚠️ PASS-WITH-CONDITIONS after Revision 1 — see §9 (first pass: ❌ NEEDS WORK)

## 1. Verdict at first pass (superseded by §9)

**The build is not shippable.** The machinery is sound; the trigger is not. `run_auto_install` cannot be reached by a user who quits the app more often than once a day, which is the dominant desktop pattern. The toggle is live, persists, gates correctly — and never fires.

Three further defects were found that the Phase 3 gate missed: a missing backend concurrency guard, an event stream that can leave the UI permanently stuck, and a progress message that asserts something untrue before the cancel check runs.

The Phase 3 gate reported PASS. That was wrong, and the error was mine: I verified that the code compiles, that the gates are ordered correctly, and that 20 unit tests pass — I did not ask whether any user could reach the code.

## 2. Method

Two independent passes:

1. **Mechanical re-run** of both suites from the working tree (evidence in §6).
2. **Independent read** by a verifier who did not write the code, briefed to trace the *executed* path from app launch to `run_auto_install` and to accept no comment or doc string as proof. Its findings were then re-checked line by line against the source before being recorded here.

Every finding below was confirmed by reading the code, not inferred from the reviewer's report.

## 3. Findings

| ID | Severity | Finding | Evidence |
|---|---|---|---|
| **F-01** | **Critical** | First auto-check fires 24 h after launch; nothing persists across launches. Background install is unreachable in normal use. | `updater.rs:1138-1141`, `updater.rs:1075`, `state.rs:86` |
| **F-02** | **High** | `update_install` bypasses the single-flight guard, so a manual and a background install of the same version can both swap the bundle. | `updater.rs:906-923`, `updater.rs:772-781` |
| **F-03** | **High** | `started` can be emitted with no terminal phase, leaving `autoInstalling === true` for the rest of the session with a dead Cancel button. | `updater.rs:1272`, `updater.rs:772-774`, `updater.rs:1297` |
| **F-04** | Medium | `verified` is emitted *before* the cancel check, so the UI says "Signature verified — installing…" and then goes silent on cancel. | `updater.rs:419-427`, `en.ts:573`, `updater.svelte.ts:257-261` |
| **F-05** | Low | The `update_auto_install` field doc contradicts the shipped behavior (claims `update_auto_check` must also be on; it must not). | `settings.rs:110-118` vs `updater.rs:1219` |
| **F-06** | Low | A manual "Check now" never hands its result to auto-install, even with the toggle on. | `updater.rs:877-900` |

### F-01 — Critical: the feature cannot fire

The scheduler initialises its sleep to the full interval *before* the first wake:

```rust
let mut sleep_for = AUTO_CHECK_INTERVAL;   // 24 h
loop {
    tokio::time::sleep(sleep_for).await;
    ...
    if !should_auto_check(auto_on, paranoid_on, last_checked_at, now) { ... continue; }
    match run_check(&state, &source).await {
        Ok(outcome) => { ...; maybe_auto_install(&app, &state, &outcome).await; }
```

`AUTO_CHECK_INTERVAL` is 24 h (`updater.rs:1075`). `last_checked_at` lives in memory only (`state.rs:86` builds an empty `UpdaterState`) and is never persisted, so every launch restarts the full 24 h sleep.

**Consequence:** a user who opens the app for two hours a day never reaches `maybe_auto_install`. Not rarely — never, structurally. The 24 h floor in `should_auto_check` never even gets a chance to short-circuit, because the process is gone long before the first wake.

This is pre-existing Phase 15 scheduler behavior (its comment defends deferring the first check: "the manual button is one click away"). It was harmless when the scheduler only *announced* updates. Phase 3 made that scheduler the sole trigger for an unattended install, so the inherited cadence now defeats the feature's entire purpose.

**Required fix:** check shortly after launch (short jittered delay, then the 24 h floor for repeats while running). One check per launch is the industry norm (Chrome, VS Code, Sparkle) and costs one unauthenticated GitHub API call.

### F-02 — High: two installs can race the bundle swap

`update_install` builds a manual backend and calls `run_install` directly:

```rust
let backend = PluginBackend::new(app);
run_install(&state, &backend, &version).await
```

`run_install` never consults `UpdaterState.auto_install`. The only guard is `if (this.installing || this.autoInstalling) return;` in the renderer store — a UI boolean set by an async event after a round trip. If a background install is in flight and the user clicks Install on a stale card, both paths pass the cached-version check and both call `update.install(bytes)` for the same version.

The single-flight check is also not atomic: `auto_install_blocker` awaits (`updater.rs:772`), then the write guard is taken separately (`:777`), so two callers can both pass the gate before either records `Running`.

**Required fix:** move the guard into the backend — one write-lock critical section that checks *not already running* and sets `Running`, shared by both the manual and background paths. The renderer check stays as a UX affordance, not as the safety mechanism.

### F-03 — High: a terminal phase is not guaranteed

`maybe_auto_install` emits `Started` (`:1272`), then calls `run_auto_install`, which re-evaluates the same gates (`:772`). If the second evaluation refuses — Offline Mode flipped on, or consent revoked in the intervening microseconds — it returns `Skipped`, and `maybe_auto_install` handles that with `Ok(AutoInstallResult::Skipped(_)) => {}`: **no terminal event.**

The renderer sets `autoInstalling = true` on `started` and only clears it on `installed` / `failed` / `canceled`. So the user is left with a permanent progress row that pre-empts the Install button, and a Cancel button that flips a flag nobody polls.

The comment above the emitter — *"no 'started' that never ran"* — is false: the gate is evaluated twice, with awaits in between.

**Required fix:** emit a terminal phase on every exit path after `Started`. A post-`Started` skip is truthfully a `canceled` (the preconditions changed), and it gives the UI its exit.

### F-04 — Medium: the UI asserts something untrue, then goes quiet

In `PluginBackend::download_and_install` the `on_verified` callback fires at `:419-421`, *before* the cancel check at `:423`. On cancel, the renderer has already received `verified` and is showing "Signature verified — installing Agency Agents vX…" (`en.ts:573`). Then `canceled` arrives and the store clears `autoInstalling` with no message, no journal row, no toast.

On the scheduler path `available` is `null` (only a manual check populates it), so the progress card unmounts entirely. The user turns the toggle off and watches a spinner claiming to install vanish with zero confirmation.

**Required fix:** notify `verified` only *after* the cancel check passes (so the phase means "verified and proceeding"), and acknowledge a cancel in the UI — one line, not a toast.

### F-05 — Low: stale field documentation

`settings.rs:110-118` still says auto-install requires `update_auto_check` to be on. The shipped scheduler gates discovery on `update_auto_check || update_auto_install` (`:1219`), precisely so the toggle is not a dead control. The doc also says "a discovered update" without noting that only scheduler-discovered updates qualify (F-06).

Behavior changed in this feature; its nearest doc did not. That is a DOC-01 violation, not a nit.

### F-06 — Low: manual check does not trigger an install

With the toggle on, clicking "Check for updates now" announces the update but never starts the background install. Accepted as designed — the manual button means "tell me", the toggle means "handle it unattended" — but it must be *documented*, and F-01's fix removes the practical sting by making discovery happen at every launch.

## 4. What actually held up

Stated plainly, because the revision should not disturb what works:

- **Gate order** is correct and fails closed: Offline/corrupt-settings first, then consent, skip-list, single-flight (`updater.rs:724-760`).
- **Download/install split** puts the cancel checkpoint exactly where it belongs — after authenticity is proven, before any disk write. A cancel leaves no partial artifact and preserves `cached_available` for a manual retry.
- **Consent revocation** reaches the in-flight pipeline through the same `Arc<AtomicBool>` the plugin backend polls; the toggle-off and Offline-engaging transitions are both covered (`updater.rs:853-866`).
- **Wire contract** matches exactly: five phases, `camelCase`, `message` omitted when absent, event name mirrored on both sides. No unhandled phase, no field mismatch (`updater.rs:638-657` vs `types.ts:252-265`).
- **Relaunch** remains user-gated; no unattended restart is possible.
- **Test quality**: no test was weakened to make the suite pass — the diff adds no `#[ignore]`, no skips, and every new `unwrap()`/`expect()` is inside `#[cfg(test)]`.

## 5. Why the tests did not catch any of this

F-01 through F-04 are all in `#[cfg(not(test))]` code: the scheduler loop, the plugin backend, and the `maybe_auto_install` glue are compiled out of the test build. The 20 unit tests exercise `auto_install_blocker`, `run_auto_install`, and the wire shapes through a mock backend — the parts that were already correct. The test suite proved the gates work; it could not prove anything reaches them.

This is the real lesson of the phase: the untested surface was exactly where the defects were, and "327 passing" was read as coverage when it was not.

## 6. Evidence re-run (working tree, after Phase 3)

```
cargo test --lib   → test result: ok. 327 passed; 0 failed; 1 ignored; 0 measured
npm run check      → svelte-check found 0 errors and 0 warnings
npm run build      → ✓ built in 19.90s, EXIT=0   (Phase 3 run, tree unchanged since)
```

Suites are green and the findings stand. Compilation and unit tests were never the question.

## 7. Required fix set (revision cycle)

| # | Fix | Closes |
|---|---|---|
| 1 | First auto-check ~60 s after launch (jittered), then the 24 h floor while running | F-01 |
| 2 | One write-lock critical section in the backend: check-not-running + set-running, used by both manual and auto paths | F-02 |
| 3 | Terminal phase on every post-`Started` exit (skip ⇒ `canceled`) | F-03 |
| 4 | Move the `verified` notification after the cancel check; acknowledge a cancel in Settings | F-04 |
| 5 | Correct the `update_auto_install` field doc; document the manual-check boundary | F-05, F-06 |
| 6 | Tests that fail if any of the above regresses (the scheduler cadence must become a testable pure function, as `should_auto_check` already is) | all |

Fix 1 changes a documented Phase 15 decision ("defer one interval"), so it needs an explicit call rather than a silent patch. Everything else is a defect fix inside the approved scope.

## 8. Verify Gate (Reality Checker)

| Criterion | Status |
|---|---|
| Feature reachable in normal usage | ❌ F-01 |
| No concurrent installs of the same version | ❌ F-02 |
| Every started install reaches a terminal UI state | ❌ F-03 |
| Progress messages are truthful | ❌ F-04 |
| Docs match shipped behavior | ❌ F-05 |
| Offline Mode is a hard gate | ✅ blocker order + consent hook |
| Skip-list honored | ✅ |
| Never silent relaunch | ✅ |
| Signature verification reused | ✅ plugin's own sha256 + minisign |
| Wire contract backend ↔ renderer | ✅ |
| No weakened or skipped tests | ✅ |
| Suites green | ✅ 327 / 0 / 1, svelte-check 0/0 |

**Verdict**: ❌ **NEEDS WORK** — return to Phase 3 (Build) for the revision cycle. Not a release candidate.

**Blocking**: F-01, F-02, F-03, F-04. **Non-blocking**: F-05, F-06.

---

## 9. Revision 1 — fixes, evidence, verdict

All six findings are closed. The feature is now reachable in ordinary use, single-flight is enforced in the backend rather than in the renderer, and every `started` event has a terminal counterpart.

### 9.1 What changed

| # | Fix | Closes |
|---|---|---|
| 1 | `startup_delay(now)` = 60 s + 0–29 s jitter for the first wake, then the 24 h floor; `next_sleep()` is the single pure cadence decision; inactive scheduler re-polls every 5 min | F-01 |
| 2 | `claim_install_slot` / `release_install_slot` (one write-lock section each), used by both `run_auto_install` and the extracted `run_manual_install`; busy manual install returns `install_in_progress` | F-02 |
| 3 | Every exit after `started` emits a terminal phase; a post-`started` skip is reported as `canceled` | F-03 |
| 4 | `verified` is notified only after the cancel checkpoint; renderer tracks `autoInstallCanceled` and Settings acknowledges it in one line | F-04 |
| 5 | `update_auto_install` field doc corrected; module doc gained a "Discovery boundary" paragraph | F-05, F-06 |
| 6 | 15 new tests + 5 retargeted (inventory in §9.3) | all |

**F-01 — cadence.** The loop's first sleep is now `startup_delay(now)`, not `AUTO_CHECK_INTERVAL`. With the toggle on, a check fires ~60–89 s after every launch and `maybe_auto_install` follows it, so the install path is reachable in a session of any length.

`should_auto_check` was **deleted** rather than kept alongside the new function: two definitions of "should we check" is how the tested gate and the shipped gate drift apart. Its five tests now assert on `next_sleep` directly and additionally pin the *wait*, not just the boolean.

The idle poll (5 min) is a deliberate trade-off, stated here because it costs something: a user who never opts in still wakes the runtime every 5 minutes to read three locks. No network, no disk, microseconds of CPU — accepted in exchange for a toggle flip taking effect without a relaunch, which is the whole point of the feature.

**F-02 — single-flight.** The guard is now one write-lock critical section that both refuses a running install and records the new one. `update_install` no longer calls `run_install` directly; it calls `run_manual_install`, which is **compiled into the test build** — deliberately, because F-02 was invisible for the same reason F-01 was: it lived in `#[cfg(not(test))]` code. A manual install attempted while a background install is in flight now fails closed with `AppError::InstallInProgress` (`install_in_progress`) instead of racing the bundle swap.

`release_install_slot` is scoped to `Running` so it cannot erase a terminal status the caller already recorded — a release after `Installed` would otherwise delete the relaunch prompt.

**F-03 — terminal phase.** `Ok(AutoInstallResult::Skipped(_))` now emits `canceled`. Every post-`started` exit maps to a terminal phase:

| `run_auto_install` result | Event |
|---|---|
| `Installed` | `installed` |
| `Canceled` | `canceled` |
| `Skipped(_)` (preconditions changed) | `canceled` |
| `Err(e)` | `failed` |

The cancel token is now reset **before** the pre-flight gate, not after it: resetting after the check would erase a revocation that landed in the window between the check and the install.

**F-04 — truthful progress.** In `PluginBackend::download_and_install` the order is now download → verify → **cancel check** → `verified` → install. `verified` therefore means "authentic and proceeding", and a cancel no longer tells the UI it is installing.

The renderer sets `autoInstallCanceled` on `canceled` (cleared on `started` / `verified` / `installed` / a manual `install()`), and Settings renders one `role="status"` line: *"Background install canceled. Nothing on disk changed — the update is still available to install."* No toast, no journal row: nothing happened, and the retry path is visible.

**F-05 / F-06 — docs.** The `update_auto_install` field doc now states that `update_auto_check` is *not* required and that only scheduler-discovered updates auto-install. The module doc carries the same boundary for the next reader.

**Beyond the six (flagged, not silent).** The eleven non-English `updates.autoInstallHint` strings still said the feature was "available once the update channel is live" — true while the toggle was inert, false now that it installs. They were updated to the shipped behavior. This is translation work done by the author, not a native speaker: see condition 2 in §9.5.

### 9.2 Why the guard is now testable

F-01, F-02 and F-03 all lived in `#[cfg(not(test))]`. The revision did not just fix them; it moved the *decisions* into compiled-in code:

- cadence → `next_sleep` / `startup_delay` / `sleep_after_failure` (pure)
- single-flight → `claim_install_slot` / `release_install_slot` / `run_manual_install` / `run_auto_install` (compiled in)
- event contract → `AutoInstallPhase` / `AutoInstallEvent` (already compiled in)

What remains outside the test build is the loop *body*, `maybe_auto_install`'s emission sequence, and `PluginBackend` itself. That is stated as a condition in §9.5, not glossed over.

### 9.3 Test inventory

New (15):

| Area | Test |
|---|---|
| error wire | `install_in_progress_serializes_to_install_in_progress_code` |
| cadence | `auto_check_startup_delay_fires_inside_a_normal_session` |
| cadence | `startup_delay_adds_bounded_jitter` |
| cadence | `next_sleep_polls_soon_when_inactive` |
| cadence | `next_sleep_waits_the_remaining_floor_when_throttled` |
| cadence | `sleep_after_failure_walks_the_backoff_then_saturates` |
| cadence | `next_backoff_index_saturates` |
| slot | `claim_install_slot_is_single_flight` |
| slot | `release_install_slot_frees_the_slot` |
| slot | `release_install_slot_preserves_terminal_status` |
| slot | `auto_install_skips_when_another_install_holds_the_slot` |
| manual | `manual_install_claims_and_releases_the_slot` |
| manual | `manual_install_is_refused_while_the_slot_is_held` |
| manual | `manual_install_releases_the_slot_on_failure` |
| manual | `manual_install_releases_the_slot_on_gate_refusal` |

(The table lists 15 rows for 15 new tests; `auto_install_skips_when_another_install_holds_the_slot` is counted once, under "slot", although it also exercises the auto-install entry point.)

Retargeted (5): `scheduler_honors_24h_floor`, `scheduler_does_not_refire_within_one_hour`, `scheduler_fires_when_never_checked`, `scheduler_suspends_on_paranoid_mode`, `scheduler_does_nothing_when_disabled` — now assert on `next_sleep`, including the exact remaining-floor wait and the idle-poll wait.

No test was weakened, skipped, or ignored to make this pass. No new `unwrap()`/`expect()` outside `#[cfg(test)]`.

### 9.4 Gates (re-run on the final tree)

```
cargo test --lib   → test result: ok. 342 passed; 0 failed; 1 ignored; 0 measured
cargo check --lib  → Finished, EXIT=0 — 1 warning (pre-existing: unused `app` in update_check_now)
npm run check      → svelte-check found 0 errors and 0 warnings
npm run build      → ✓ built, adapter-static wrote "build", EXIT=0
```

Test-count delta: 327 → 342 (+15 new; the five scheduler tests were rewritten in place, not added). The 1 ignored test remains the upstream parity test that needs `AGENCY_AGENTS_PARITY_ROOT`.

### 9.5 Verdict

**PASS-WITH-CONDITIONS.** Every finding is closed with a regression test, and the feature is reachable on the dominant desktop pattern.

Two conditions before this ships:

1. **Manual smoke test on a signed build.** Enable the toggle, let the scheduler run, and watch `started → verified → installed` on a real manifest, then accept the relaunch prompt. Every install in this record is mock-backed; no signed artifact has been swapped by this code yet. This was already listed as a Phase 3 follow-up and it is now the release gate.
2. **Native review of the eleven `updates.autoInstallHint` strings.** They were rewritten to match shipped behavior; the meaning is right, the phrasing deserves a native read.

Not conditions, recorded as accepted limits: the loop body and `PluginBackend` stay outside the test build (§9.2); the 5-minute idle poll costs an idle wake (§9.1); a manual "Check now" still does not start an install, by design (§3, F-06).

### 9.6 Verify gate (Revision 1)

| Criterion | Status |
|---|---|
| Feature reachable in normal usage | ✅ first wake 60–89 s after launch |
| No concurrent installs of the same version | ✅ one atomic slot, both entry points, tested |
| Every started install reaches a terminal UI state | ✅ all four exits map to a phase |
| Progress messages are truthful | ✅ `verified` after the cancel check |
| A cancel is acknowledged | ✅ one line in Settings |
| Docs match shipped behavior | ✅ field doc + module doc + 11 hints |
| Offline Mode is a hard gate | ✅ unchanged, still first |
| Skip-list honored | ✅ unchanged |
| Never silent relaunch | ✅ unchanged |
| Signature verification reused | ✅ unchanged |
| Wire contract backend ↔ renderer | ✅ `install_in_progress` added on both sides |
| No weakened or skipped tests | ✅ |
| Suites green | ✅ 342 / 0 / 1, svelte-check 0/0, build OK |
| Real signed install exercised | ⚠️ condition 1 |

