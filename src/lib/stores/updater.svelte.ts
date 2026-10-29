/**
 * Updater store (Phase 15).
 *
 * Mirrors backend `update_*` commands into the renderer. Two consumers:
 *
 *   - `UpdateIndicator.svelte` — the title-bar pill. Renders only when
 *     `available !== null` AND Offline Mode is off (the second gate is
 *     evaluated in the component against `settings.effective.paranoidMode`).
 *   - `SettingsSectionUpdates.svelte` — the Settings → Network →
 *     Updates subsection. Owns the manual "Check for updates now"
 *     button, the auto-check toggle, and the install action.
 *
 * Background auto-install (Phase 3) is **observed**, not driven: the
 * backend scheduler owns the install and emits `update:auto-install`
 * phases, which `attachAutoInstallListener()` folds into this store.
 * That split is what lets an install complete while the user is on
 * another view — the renderer is a spectator, so navigating away (or
 * never opening Settings) cannot interrupt it.
 *
 * **No update info on disk in this store.** The skip-list, last-checked
 * timestamp, and the cached "available" entry all live on the backend
 * inside `settings.json`. This store just shadows the most recent
 * outcome so the UI doesn't have to re-IPC on every render.
 *
 * Failure modes handled here (everything else propagates to the caller):
 *
 *   - `paranoid_mode_blocked` → swallowed silently into `available = null`
 *     because the indicator must hide in Offline Mode anyway, and the
 *     Settings card shows its own "Disabled by Offline Mode" tooltip
 *     on the button.
 *   - any other `AppError` → stored on `error` for the Settings card
 *     to surface inline with a "Try again" button.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  updateAutoInstallCancel,
  updateCheckNow,
  updateInstall,
  updateRelaunch,
  updateSkip,
} from "$lib/api";
import {
  isAppError,
  type AutoInstallEvent,
  type AutoInstallPhase,
  type UpdateInfo,
  errorText,
} from "$lib/types";
import { activity } from "$lib/stores/activity.svelte";
import { i18n } from "$lib/stores/i18n.svelte";

/**
 * Mirrors `AUTO_INSTALL_EVENT` in `src-tauri/src/commands/updater.rs`.
 * A rename on either side silently disables the whole background-install
 * UI, so the two constants are pinned together by comment.
 */
const AUTO_INSTALL_EVENT = "update:auto-install";

class UpdaterStore {
  /** Epoch millis of the most recent check (success OR error). `null`
      until the first check completes. Surfaced in the Settings card as
      a "Last checked: …" line for the curious user. */
  lastChecked: number | null = $state(null);

  /** The currently-known available update, or `null` when up-to-date /
      blocked / never-checked. Reactive — the title-bar indicator's
      visibility tracks the transition from `null` ↔ a value. */
  available: UpdateInfo | null = $state(null);

  /** True while `checkNow()` is in flight. The Settings button uses
      this to render a spinner inline. */
  checking: boolean = $state(false);

  /** True while `install()` is in flight. The Settings card swaps
      its install button for a progress indicator; the title-bar
      indicator swaps its × for a small spinner so the user sees
      activity even when they navigated away from Settings. */
  installing: boolean = $state(false);

  /** True after a successful install. Surfaced as a "Relaunch now"
      affordance in the Settings card. The Settings card's button
      calls `relaunch()` which fires the `update_relaunch` IPC; the
      backend schedules `tauri::AppHandle::restart()` on a short
      delay so the IPC response arrives before the process dies. */
  installComplete: boolean = $state(false);

  /** Human-readable error from the most recent op (`checkNow`,
      `install`, or a failed background auto-install), or `null`.
      Cleared by the next successful op. */
  error: string | null = $state(null);

  /** True between the backend's `started` event and a terminal phase.
      Drives the progress row in Settings and the title-bar spinner so
      a background install is visible from any view. */
  autoInstalling: boolean = $state(false);

  /** Version the background install is working on, or `null` before the
      first event. */
  autoInstallVersion: string | null = $state(null);

  /** Most recent phase reported by the backend, or `null` before the
      first event. Kept for the Settings card's "verifying…" wording. */
  autoInstallPhase: AutoInstallPhase | null = $state(null);

  /** True after a background install ended in `canceled`: consent revoked,
      Offline Mode engaged, or the Cancel button. Drives a one-line
      acknowledgement in Settings — without it the spinner simply disappears and
      the user cannot tell whether the cancel worked or the install failed.
      Cleared when the next install attempt starts. */
  autoInstallCanceled: boolean = $state(false);

  /** Unlisten handle for the event subscription. Held so a hot reload
      cannot stack duplicate listeners (each duplicate would append a
      second Activity Journal row per install). */
  private unlistenAutoInstall: UnlistenFn | null = null;

  /**
   * Manually check the manifest. Updates `available` based on the
   * outcome; the title-bar indicator's visibility falls out of that.
   * Idempotent — safe to spam-click the Settings button.
   *
   * Maps `paranoid_mode_blocked` to a silent `available = null` because
   * Offline Mode hides the indicator anyway and the Settings button
   * surfaces its own disabled-state tooltip.
   */
  async checkNow(): Promise<void> {
    if (this.checking) return;
    this.checking = true;
    this.error = null;
    try {
      const outcome = await updateCheckNow();
      this.lastChecked = Date.now();
      switch (outcome.kind) {
        case "available":
          this.available = {
            version: outcome.version,
            currentVersion: outcome.currentVersion,
            notes: outcome.notes,
            pubDate: outcome.pubDate,
            releaseUrl: outcome.releaseUrl,
            installable: outcome.installable,
            skipped: outcome.skipped,
          };
          break;
        case "upToDate":
          this.available = null;
          break;
      }
    } catch (e) {
      this.lastChecked = Date.now();
      if (isAppError(e) && e.code === "paranoid_mode_blocked") {
        // Hide the indicator; don't surface a confusing error in
        // the Settings card (the button's own tooltip explains why).
        this.available = null;
        this.error = null;
      } else {
        this.error = errorText(e);
      }
    } finally {
      this.checking = false;
    }
  }

  /**
   * Download + verify + install the named version. The backend
   * cross-checks `version` against the cached "available" entry, so
   * passing a stale version fast-fails with `invalid_argument`.
   *
   * On success: flips `installComplete = true` and leaves `available`
   * intact so the Settings card can render its "Relaunch now"
   * affordance. The title-bar indicator continues to render until
   * the user relaunches or dismisses.
   *
   * On failure: stashes the message on `error` and clears the
   * `installing` flag so the Settings card can show its retry path.
   */
  async install(version: string): Promise<void> {
    // A background install is already working on this same version. Two
    // concurrent installs would race the bundle swap, so the second one is
    // dropped rather than queued. The UI never offers the button during an
    // auto-install (the card shows progress + Cancel instead), so this is a
    // guard against a stale click, not a user-facing path.
    if (this.installing || this.autoInstalling) return;
    this.installing = true;
    this.installComplete = false;
    this.autoInstallCanceled = false;
    this.error = null;
    try {
      await updateInstall(version);
      this.installComplete = true;
    } catch (e) {
      this.error = errorText(e);
    } finally {
      this.installing = false;
    }
  }

  /**
   * Subscribe to the backend's background-install events. Called once from
   * `+layout.svelte` so the subscription outlives every view.
   *
   * Idempotent: a second call returns immediately, so a hot reload cannot
   * stack listeners.
   */
  async attachAutoInstallListener(): Promise<void> {
    if (this.unlistenAutoInstall) return;
    try {
      this.unlistenAutoInstall = await listen<AutoInstallEvent>(
        AUTO_INSTALL_EVENT,
        (event) => {
          this.handleAutoInstallEvent(event.payload);
        },
      );
    } catch (e) {
      // No Tauri event bus (plain-browser `vite dev`, or a listener rejected
      // during shutdown). Background installs then simply go unobserved by
      // this renderer — the backend still installs, and the Settings card's
      // manual path remains usable on the next launch.
      console.warn(
        `[updater] auto-install listener unavailable: ${
          e instanceof Error ? e.message : String(e)
        }`,
      );
    }
  }

  /** Release the subscription. Safe to call when nothing is attached. */
  detachAutoInstallListener(): void {
    this.unlistenAutoInstall?.();
    this.unlistenAutoInstall = null;
  }

  /**
   * Fold one backend phase into the store. Exposed (not private) so the
   * phase → UI mapping can be exercised without a live Tauri event bus.
   *
   * Terminal phases are the only ones that touch the Activity Journal: a
   * `started`/`verified` pair is progress, not history.
   */
  handleAutoInstallEvent(payload: AutoInstallEvent): void {
    this.autoInstallVersion = payload.version;
    this.autoInstallPhase = payload.phase;

    switch (payload.phase) {
      case "started":
      case "verified":
        this.autoInstalling = true;
        this.autoInstallCanceled = false;
        this.error = null;
        break;

      case "installed":
        this.autoInstalling = false;
        this.autoInstallCanceled = false;
        this.installComplete = true;
        // The version is installed — leaving it in `available` would make
        // the Settings card offer an install that already happened.
        this.available = null;
        activity.log({
          action: "appUpdate",
          outcome: "ok",
          detail: i18n.t("updates.journalInstalled", { version: payload.version }),
        });
        break;

      case "canceled":
        // No error and no journal row: nothing was written, and the backend
        // keeps `cached_available`, so the manual install path is still open.
        // The flag exists so the UI can say so in one line instead of letting
        // the progress row vanish without a word.
        this.autoInstalling = false;
        this.autoInstallCanceled = true;
        break;

      case "failed":
        this.autoInstalling = false;
        this.autoInstallCanceled = false;
        this.error = payload.message ?? i18n.t("updates.autoInstallFailed");
        activity.log({
          action: "appUpdate",
          outcome: "error",
          detail: this.error,
        });
        break;
    }
  }

  /**
   * Ask the backend to abort the in-flight background install. Cooperative:
   * the backend checks the flag between signature verification and the bundle
   * swap, so a cancel that arrives later is a harmless no-op and the update
   * simply installs.
   */
  async cancelAutoInstall(): Promise<void> {
    if (!this.autoInstalling) return;
    try {
      await updateAutoInstallCancel();
    } catch (e) {
      this.error = errorText(e);
    }
  }

  /**
   * Add `version` to the backend's skip-list. The title-bar indicator
   * hides as soon as `available` flips to `null`; a future release
   * (any version newer than this one) re-triggers the indicator.
   *
   * Optimistic — clears `available` before the IPC resolves so the
   * pill disappears immediately. The skip-list write itself is
   * eventually consistent on the backend; if the IPC fails (e.g.
   * Offline Mode flipped on between the indicator render and the ×
   * click, blocking the settings write) the indicator stays hidden
   * for this session and re-checks on the next launch.
   */
  async skip(version: string): Promise<void> {
    // Optimistic update — indicator disappears immediately.
    this.available = null;
    try {
      await updateSkip(version);
    } catch (e) {
      // Best-effort: don't restore the indicator on failure (the user
      // explicitly asked to dismiss; better to keep their click than
      // surface a confusing "we couldn't dismiss" toast).
      this.error = errorText(e);
    }
  }

  /**
   * Relaunch the app after a successful install. Fires the backend's
   * `update_relaunch` IPC, which schedules `tauri::AppHandle::restart()`
   * on a short delay so the IPC response arrives before the process
   * dies. The renderer's pending `await` is expected to be torn down
   * mid-call — we don't wait on it.
   */
  async relaunch(): Promise<void> {
    try {
      await updateRelaunch();
    } catch (e) {
      // The process may have already started restarting and the IPC
      // socket closed mid-call. Treat any error here as benign — if
      // the restart actually failed, the user will notice immediately.
      this.error = errorText(e);
    }
  }

  /** Reset post-install / post-error UI to its baseline. Useful from
      the Settings card when the user dismisses an error and wants to
      check again. */
  clearError(): void {
    this.error = null;
  }
}

/** Module-level singleton. Components import { updater } and read
    `updater.available`, etc. as Svelte 5 reactive state. */
export const updater = new UpdaterStore();
