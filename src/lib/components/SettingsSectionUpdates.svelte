<script lang="ts">
  /**
   * SettingsSectionUpdates.svelte — Phase 15
   *
   * Mounted at the bottom of `SettingsSectionNetwork.svelte` (the
   * Updates flow IS a network feature, so it lives in the same pane
   * the user already opens to manage outbound posture).
   *
   * Three always-visible rows:
   *   1. "Check for updates now" button — disabled with the
   *      "Disabled by Offline Mode" tooltip when Offline Mode is on.
   *   2. "Auto-check daily" toggle — bound to
   *      `settings.updateAutoCheck`. Stays toggleable even in Offline
   *      Mode so the user can configure their preference for the next
   *      time Offline Mode is off (matches every other network toggle).
   *   3. "Update channel: Stable" — read-only display, placeholder
   *      for v0.4+ beta channel support.
   *
   * One conditional card (when `updater.available !== null`):
   *   - **v{version} available** heading
   *   - "Release notes ↗" link (opens via `safeOpenUrl`)
   *   - [Install update] button → `updater.install(version)` → swaps to
   *     a progress indicator while installing, then to a success line
   *     + "Relaunch now" button on completion. On failure: inline
   *     error text + "Try again" button.
   *
   * No "Skip this version" button here — the × on the title-bar
   * indicator owns the skip path so the Settings card stays focused on
   * positive actions.
   */

  import Download from "@lucide/svelte/icons/download";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Loader from "@lucide/svelte/icons/loader-2";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import CheckCircle from "@lucide/svelte/icons/check-circle-2";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import X from "@lucide/svelte/icons/x";

  import { settings } from "$lib/stores/settings.svelte";
  import { updater } from "$lib/stores/updater.svelte";
  import { safeOpenUrl } from "$lib/util/url";
  import { i18n } from "$lib/stores/i18n.svelte";
  import DestructiveConfirm from "$lib/components/DestructiveConfirm.svelte";

  /** Offline Mode gates both the manual button and the Install action.
      Tracked as a single derived for tidy template gating. */
  let offline = $derived(settings.effective.paranoidMode);

  /** First-time consent dialog for the auto-install opt-in. The toggle
      is a stronger consent than auto-check, so enabling it the first
      time asks for explicit confirmation. */
  let consentOpen = $state(false);

  /** Available-update snapshot for the conditional card. */
  let info = $derived(updater.available);

  function onCheckNow() {
    void updater.checkNow();
  }

  function onToggleAutoCheck(e: Event) {
    const v = (e.currentTarget as HTMLInputElement).checked;
    void settings.save({ updateAutoCheck: v });
  }

  /** Enabling auto-install for the first time opens a consent dialog.
      Disabling is immediate — revoking consent never needs confirmation. */
  function onToggleAutoInstall(e: Event) {
    const v = (e.currentTarget as HTMLInputElement).checked;
    if (v && !settings.effective.updateAutoInstall) {
      consentOpen = true;
      return;
    }
    void settings.save({ updateAutoInstall: v });
  }

  function onConfirmAutoInstall() {
    consentOpen = false;
    void settings.save({ updateAutoInstall: true });
  }

  function onCancelConsent() {
    consentOpen = false;
  }

  function onInstall() {
    if (!info) return;
    void updater.install(info.version);
  }

  function onRelaunch() {
    void updater.relaunch();
  }

  function onTryAgain() {
    updater.clearError();
    if (info) void updater.install(info.version);
  }

  /** Always-correct release notes URL derived from the version tag.
      The manifest's free-form `notes` field is rendered inline below;
      this link goes to the canonical GH release page so users can
      see the changelog + assets even when the manifest snippet is
      terse. */
  let releaseNotesUrl = $derived(info?.releaseUrl ?? null);

  function onOpenReleaseNotes() {
    if (releaseNotesUrl) void safeOpenUrl(releaseNotesUrl);
  }

  /** Format the last-checked timestamp for the curious user. Locale-
      sensitive; falls back to "—" when never checked. */
  let lastCheckedLabel = $derived.by(() => {
    if (updater.lastChecked === null) return "—";
    return new Date(updater.lastChecked).toLocaleString();
  });

  /** Version shown in the progress row: the background install's target
      when one is running, otherwise the manually-selected update. */
  let progressVersion = $derived(updater.autoInstallVersion ?? info?.version ?? "");

  /** Progress copy. A background install reads differently from a manual one
      ("in the background" vs. a click the user just made), and `verified`
      earns its own wording: the artifact is downloaded and signature-checked,
      so the remaining wait is the bundle swap. Only an auto install can be in
      that phase — a manual install leaves `autoInstallPhase` at its last value. */
  let progressLabel = $derived.by(() => {
    const version = progressVersion;
    if (!updater.autoInstalling) {
      return i18n.t("updates.downloading", { version });
    }
    if (updater.autoInstallPhase === "verified") {
      return i18n.t("updates.autoInstallVerifying", { version });
    }
    return i18n.t("updates.autoInstalling", { version });
  });

  function onCancelAutoInstall() {
    void updater.cancelAutoInstall();
  }
</script>

<div class="section">
  <h2>{i18n.t("updates.title")}</h2>

  <!-- Row 1: Check for updates now -->
  <div class="field">
    <span class="field-label">{i18n.t("updates.checkNowLabel")}</span>
    <div class="row">
      <button
        type="button"
        class="btn-secondary"
        onclick={onCheckNow}
        disabled={offline || updater.checking}
        title={offline
          ? i18n.t("common.disabledByOffline")
          : i18n.t("updates.checkManifestTitle")}
      >
        {#if updater.checking}
          <span class="spin"><Loader size={14} /></span>
          {i18n.t("updates.checking")}
        {:else}
          <RefreshCw size={14} />
          {i18n.t("updates.checkNow")}
        {/if}
      </button>
      <span class="meta"
        >{i18n.t("updates.lastChecked", { time: lastCheckedLabel })}</span
      >
    </div>
    {#if offline}
      <p class="hint">{i18n.t("updates.offlineHint")}</p>
    {:else}
      <p class="hint">{i18n.t("updates.onlineHint")}</p>
    {/if}
  </div>

  <!-- Row 2: Auto-check daily -->
  <div class="field">
    <label class="toggle">
      <input
        type="checkbox"
        checked={settings.effective.updateAutoCheck}
        onchange={onToggleAutoCheck}
        disabled={settings.loading || settings.corruptOnDisk}
      />
      <span class="toggle-track" aria-hidden="true"></span>
      <span class="toggle-label">{i18n.t("updates.autoCheck")}</span>
    </label>
    <p class="hint">{i18n.t("updates.autoCheckHint")}</p>
  </div>

  <!-- Row 2b: Install updates automatically. A stronger opt-in than
       auto-check: enabling it for the first time opens a consent dialog.
       The runtime gate still honours Offline Mode and the skip-list. -->
  <div class="field">
    <label class="toggle">
      <input
        type="checkbox"
        checked={settings.effective.updateAutoInstall}
        onchange={onToggleAutoInstall}
        disabled={settings.loading || settings.corruptOnDisk}
        aria-describedby="auto-install-hint"
      />
      <span class="toggle-track" aria-hidden="true"></span>
      <span class="toggle-label">{i18n.t("updates.autoInstall")}</span>
    </label>
    <p class="hint" id="auto-install-hint">
      {i18n.t("updates.autoInstallHint")}
    </p>
  </div>

  <!-- Row 3: Update channel -->
  <div class="field">
    <span class="field-label">{i18n.t("updates.channel")}</span>
    <div class="channel-row">
      <span class="channel-name">{i18n.t("updates.stable")}</span>
      <span class="meta">{i18n.t("updates.noBeta")}</span>
    </div>
  </div>

  <!-- Conditional: notice card when an update is available -->
  {#if info}
    <div
      class="notice"
      role="region"
      aria-label={i18n.t("updates.availableAria", { version: info.version })}
    >
      <div class="notice-head">
        <strong>{i18n.t("updates.available", { version: info.version })}</strong
        >
        <button
          type="button"
          class="link"
          onclick={onOpenReleaseNotes}
          aria-label={i18n.t("common.openReleaseNotes")}
        >
          {i18n.t("updates.releaseNotes")}
          <ExternalLink size={12} />
        </button>
      </div>

      {#if updater.installComplete}
        <div class="result success">
          <CheckCircle size={16} />
          <span>{i18n.t("updates.installComplete")}</span>
        </div>
        <button
          type="button"
          class="btn-primary"
          onclick={onRelaunch}
          title={i18n.t("updates.relaunchTitle")}
        >
          <RotateCw size={14} />
          {i18n.t("common.relaunchNow")}
        </button>
      {:else if updater.installing || updater.autoInstalling}
        <div class="progress" role="status" aria-live="polite">
          <span class="spin"><Loader size={16} /></span>
          <span>{progressLabel}</span>
        </div>
        {#if updater.autoInstalling}
          <button
            type="button"
            class="btn-secondary"
            onclick={onCancelAutoInstall}
            title={i18n.t("updates.cancelAutoInstallTitle")}
          >
            <X size={14} />
            {i18n.t("updates.cancelAutoInstall")}
          </button>
        {/if}
      {:else if updater.error}
        <div class="result error">
          <TriangleAlert size={16} />
          <span>{updater.error}</span>
        </div>
        <button
          type="button"
          class="btn-secondary"
          onclick={onTryAgain}
          disabled={offline}
          title={offline
            ? i18n.t("common.disabledByOffline")
            : i18n.t("updates.retryTitle")}
        >
          <RotateCw size={14} />
          {i18n.t("common.tryAgain")}
        </button>
      {:else if !info.installable}
        <button
          type="button"
          class="btn-secondary"
          onclick={onOpenReleaseNotes}
          disabled={offline}
          title={i18n.t("updates.viewReleaseTitle", { version: info.version })}
        >
          <ExternalLink size={14} />
          {i18n.t("updates.viewRelease")}
        </button>
      {:else}
        <button
          type="button"
          class="btn-primary"
          onclick={onInstall}
          disabled={offline}
          title={offline
            ? i18n.t("common.disabledByOffline")
            : i18n.t("updates.installTitle", { version: info.version })}
        >
          <Download size={14} />
          {i18n.t("updates.install")}
        </button>
      {/if}

      {#if info.notes}
        <p class="notes-body">{info.notes}</p>
      {/if}
    </div>
  {/if}

  <!-- Background auto-install with no notice card to host it. `available` is
       only populated by a manual check, so a scheduler-driven install can
       start while this pane has nothing to show. -->
  {#if updater.autoInstalling && !info}
    <div class="notice" role="status" aria-live="polite">
      <div class="progress">
        <span class="spin"><Loader size={16} /></span>
        <span>{progressLabel}</span>
      </div>
      <button
        type="button"
        class="btn-secondary"
        onclick={onCancelAutoInstall}
        title={i18n.t("updates.cancelAutoInstallTitle")}
      >
        <X size={14} />
        {i18n.t("updates.cancelAutoInstall")}
      </button>
    </div>
  {/if}

  <!-- Completion / failure from a background install, which by definition can
       land while the user is anywhere in the app (or before they ever open
       this pane). Kept outside `{#if info}` so the relaunch prompt survives
       the `available` entry being cleared by the install itself.
       `!info` guards against double-rendering on the manual path, where the
       notice card above owns both states. -->
  {#if updater.installComplete && !info}
    <div class="notice" role="status" aria-live="polite">
      <div class="result success">
        <CheckCircle size={16} />
        <span>{i18n.t("updates.installComplete")}</span>
      </div>
      <button
        type="button"
        class="btn-primary"
        onclick={onRelaunch}
        title={i18n.t("updates.relaunchTitle")}
      >
        <RotateCw size={14} />
        {i18n.t("common.relaunchNow")}
      </button>
    </div>
  {:else if updater.error && !info}
    <div class="notice" role="alert">
      <div class="result error">
        <TriangleAlert size={16} />
        <span>{updater.error}</span>
      </div>
    </div>
  {/if}

  <!-- A canceled background install needs one line of acknowledgement: the
       progress row disappears on `canceled`, and silence reads as a failure.
       Rendered outside the `{#if info}` card so it is visible whether or not a
       manual check happened to populate `available`; the install action below
       (or the card's) is the retry path. -->
  {#if updater.autoInstallCanceled && !updater.autoInstalling}
    <div class="notice" role="status" aria-live="polite">
      <div class="result">
        <X size={16} />
        <span>{i18n.t("updates.autoInstallCanceled")}</span>
      </div>
    </div>
  {/if}
</div>

<DestructiveConfirm
  open={consentOpen}
  title={i18n.t("updates.autoInstallConsentTitle")}
  confirmLabel={i18n.t("updates.autoInstallEnable")}
  confirmVariant="primary"
  onConfirm={onConfirmAutoInstall}
  onCancel={onCancelConsent}
>
  <p>{i18n.t("updates.autoInstallConsentBody")}</p>
</DestructiveConfirm>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    max-width: 580px;
    /* Mounted at the bottom of the Network section — a top divider
       reads as the section break instead of having a duplicate H2
       hug the previous content. */
    margin-top: var(--space-3);
    padding-top: var(--space-5);
    border-top: 1px solid var(--color-border);
  }
  h2 {
    font-size: var(--text-h2);
    font-weight: var(--fw-semibold);
    color: var(--color-text-primary);
    margin: 0 0 var(--space-2) 0;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .field-label {
    font-size: var(--text-body);
    font-weight: var(--fw-medium);
    color: var(--color-text-primary);
  }
  .hint {
    font-size: var(--text-body-sm);
    color: var(--color-text-muted);
    line-height: var(--lh-snug);
  }
  .row {
    display: inline-flex;
    align-items: center;
    gap: var(--space-3);
  }
  .meta {
    font-size: var(--text-body-sm);
    color: var(--color-text-muted);
  }

  /* ---------- Toggle (matches Network/GitHub) ---------- */
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    cursor: pointer;
    user-select: none;
  }
  .toggle input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .toggle-track {
    width: 36px;
    height: 20px;
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-border);
    border-radius: 999px;
    position: relative;
    transition: background-color var(--motion-duration-fast)
      var(--motion-ease-out);
  }
  .toggle-track::after {
    content: "";
    position: absolute;
    top: 1px;
    left: 1px;
    width: 16px;
    height: 16px;
    background: var(--color-surface-raised);
    border-radius: 50%;
    box-shadow: var(--shadow-xs);
    transition: transform var(--motion-duration-fast) var(--motion-ease-out);
  }
  .toggle input:checked + .toggle-track {
    background: var(--color-accent, #b8542a);
    border-color: var(--color-accent, #b8542a);
  }
  .toggle input:checked + .toggle-track::after {
    transform: translateX(16px);
    background: white;
  }
  .toggle-label {
    font-size: var(--text-body);
    font-weight: var(--fw-medium);
    color: var(--color-text-primary);
  }

  /* ---------- Channel row ---------- */
  .channel-row {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-3);
  }
  .channel-name {
    font-size: var(--text-body);
    color: var(--color-text-primary);
    font-weight: var(--fw-medium);
    padding: 2px 10px;
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  /* ---------- Buttons (match GitHub section pattern) ---------- */
  .btn-primary,
  .btn-secondary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--radius-md);
    font-size: var(--text-body-sm);
    font-weight: var(--fw-medium);
    cursor: pointer;
    width: max-content;
  }
  .btn-primary {
    background: var(--color-accent, #b8542a);
    color: white;
  }
  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.05);
  }
  .btn-primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .btn-secondary {
    background: var(--color-surface-raised);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border);
  }
  .btn-secondary:hover:not(:disabled) {
    background: var(--color-surface);
  }
  .btn-secondary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  /* ---------- Notice card (when update is available) ---------- */
  .notice {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    background: var(--color-warning-subtle, var(--color-surface-sunken));
    border: 1px solid var(--color-warning, var(--color-border));
    border-radius: var(--radius-md);
  }
  .notice-head {
    display: inline-flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
    color: var(--color-warning-on-subtle, var(--color-text-primary));
    font-size: var(--text-body);
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: none;
    color: var(--color-accent, #b8542a);
    cursor: pointer;
    font-size: var(--text-body-sm);
    padding: 0;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .link:hover {
    filter: brightness(1.1);
  }
  .link:focus-visible {
    outline: 2px solid
      var(--color-focus, var(--color-brand, var(--color-accent)));
    outline-offset: 2px;
    border-radius: var(--radius-sm);
  }

  /* In-progress / result rows live inside the notice card. */
  .progress,
  .result {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-body-sm);
    color: var(--color-text-primary);
  }
  .result.success {
    color: #2e7d32;
  }
  .result.error {
    color: var(--color-warning-strong, #b45309);
  }

  /* Release notes body — the manifest's free-form `notes` text. */
  .notes-body {
    margin: 0;
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    line-height: var(--lh-snug);
    white-space: pre-wrap;
  }

  /* Spinner used inline in buttons and progress rows. */
  .spin {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
</style>
