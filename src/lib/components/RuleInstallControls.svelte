<script lang="ts">
  /**
   * RuleInstallControls — install / uninstall for one (rule, harness, scope).
   *
   * Install state is NOT held here: it is derived from `rules.installations`,
   * which the store re-reads from the ledger after every mutation. A failure
   * leaves the previous state untouched and shows the verbatim error; it never
   * claims success (spec §10.6, TASK 21).
   *
   * Every disabled install carries a stated reason — disabled-without-a-reason
   * is a defect — and the harness is looked up in the registry so the OpenCode
   * plugin copy interpolates `plugin.package` instead of a hardcoded id (§8).
   * The reasons are ordered most-specific-first (`invalid` → `no project root`
   * → `not installable` → `preview loading` → `preview failed`), and project
   * scope with no folder renders NO install state at all: the user row carries
   * the same `project_path: null`, so resolving it would offer an Uninstall that
   * deletes the user-global install.
   */
  import UploadIcon from "@lucide/svelte/icons/upload";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import Button from "./Button.svelte";
  import DestructiveConfirm from "./DestructiveConfirm.svelte";
  import Pill from "./Pill.svelte";
  import { harnessLabel, isHarnessInstallable, ruleHarness } from "$lib/data/ruleRegistry";
  import { installBlockReason, needsProjectRoot } from "$lib/rules/derive";
  import { i18n } from "$lib/stores/i18n.svelte";
  import { rules } from "$lib/stores/rules.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { errorText, type Scope } from "$lib/types";

  interface Props {
    slug: string;
    /** Display name for the confirm/aria copy. */
    name: string;
    /** `rule.validation.valid` — an invalid rule can never be installed. */
    valid: boolean;
    harness: string | null;
    scope: Scope;
    projectPath: string | null;
    /** Absolute destination from the preview; `null` when there are no bytes. */
    dest: string | null;
    /** True while a compile for the shown `(harness, scope, root)` is in flight. */
    previewLoading: boolean;
    /** Compile failure for the shown `(harness, scope, root)`, ready to display. */
    previewError: string | null;
  }

  let {
    slug,
    name,
    valid,
    harness,
    scope,
    projectPath,
    dest,
    previewLoading,
    previewError,
  }: Props = $props();

  const meta = $derived(harness === null ? null : ruleHarness(harness));
  const label = $derived(harness === null ? "" : harnessLabel(harness));
  /** Project scope with no picked folder: nothing can be previewed or written,
   *  and there is NO install state — the user row shares the same ledger key
   *  (`project_path: null`), so resolving one here would offer an Uninstall that
   *  deletes the user-global install. */
  const needsRoot = $derived(needsProjectRoot(scope, projectPath));
  const installed = $derived(
    harness === null || needsRoot ? null : rules.installFor(slug, harness, scope, projectPath),
  );
  const locked = $derived(rules.busy !== null);

  /**
   * Why the install button is disabled, or `null` when it may run.
   *
   * Most specific case wins: an invalid rule and an unpicked project root each
   * have their own sentence, and neither may collapse into the generic
   * "this harness cannot be written" line.
   */
  const reason = $derived.by((): string | null => {
    const key = installBlockReason({
      valid,
      harness,
      installable: meta !== null && isHarnessInstallable(meta),
      scope,
      projectPath,
      previewLoading,
      previewError,
      dest,
    });
    switch (key) {
      case null:
        return null;
      case "invalid":
        return i18n.t("rules.install.reasonInvalid");
      case "noProjectRoot":
        return i18n.t("rules.install.reasonNoProjectRoot");
      case "notInstallable":
        return i18n.t("rules.install.reasonNotInstallable");
      case "previewLoading":
        return i18n.t("rules.install.reasonPreviewLoading");
      case "previewFailed":
        return i18n.t("rules.install.reasonPreviewFailed");
    }
  });

  const projectName = $derived(
    projectPath === null
      ? ""
      : (projectPath.split(/[\\/]/).filter(Boolean).pop() ?? projectPath),
  );

  let confirmInstall = $state(false);
  let confirmUninstall = $state(false);
  let acting = $state<"install" | "uninstall" | null>(null);
  /** Live-region text — always in the DOM so the change is announced. */
  let announcement = $state("");

  // The result belongs to one (rule, harness, scope) triple: clear it when the
  // triple changes so the live region never reports another rule's install.
  $effect(() => {
    void slug;
    void harness;
    void scope;
    void projectPath;
    announcement = "";
  });

  function openInstall() {
    if (reason !== null) return;
    confirmInstall = true;
  }

  async function doInstall() {
    confirmInstall = false;
    if (harness === null || reason !== null) return;
    acting = "install";
    try {
      await rules.install(slug, harness, scope, projectPath);
      announcement = i18n.t("rules.install.success", { rule: name, harness: label });
      toast.success(announcement);
    } catch (e) {
      announcement = i18n.t("rules.install.failed", {
        rule: name,
        harness: label,
        error: errorText(e),
      });
      toast.error(announcement);
    } finally {
      acting = null;
    }
  }

  async function doUninstall() {
    confirmUninstall = false;
    if (harness === null) return;
    acting = "uninstall";
    try {
      await rules.uninstall(slug, harness, scope, projectPath);
      announcement = i18n.t("rules.uninstall.success");
      toast.success(announcement);
    } catch (e) {
      announcement = i18n.t("rules.uninstall.failed", { error: errorText(e) });
      toast.error(announcement);
    } finally {
      acting = null;
    }
  }
</script>

<div class="ri">
  {#if meta?.plugin}
    <!-- Always shown, not only on first install: the app cannot know from the
         client whether the entry is already registered, and a false "nothing
         else changes" is worse than a conditional "if missing" (ux §5.2). -->
    <p class="ri-notice">
      {i18n.t("rules.install.opencodeNotice", { plugin: meta.plugin.package })}
    </p>
  {/if}

  <div class="ri-actions" aria-busy={acting !== null}>
    {#if needsRoot}
      <!-- No install state and no button: with no folder picked there is no
           destination, and the user-scope row must not be mistaken for this one.
           The same sentence the disabled button would have carried is shown. -->
      <span class="ri-state">{i18n.t("rules.install.reasonNoProjectRoot")}</span>
    {:else if installed !== null}
      <Pill tone="success">{i18n.t("rules.install.installed")}</Pill>
      <code class="ri-dest" dir="ltr">{installed.dest}</code>
      <Button
        variant="danger"
        size="md"
        loading={acting === "uninstall"}
        disabled={locked && acting !== "uninstall"}
        onclick={() => (confirmUninstall = true)}
      >
        {#snippet icon()}<TrashIcon size={14} />{/snippet}
        {acting === "uninstall" ? i18n.t("rules.uninstall.busy") : i18n.t("rules.uninstall.action")}
      </Button>
    {:else}
      <span class="ri-state">{i18n.t("rules.install.notInstalled")}</span>
      <Button
        variant="primary"
        size="md"
        loading={acting === "install"}
        disabled={reason !== null || (locked && acting !== "install")}
        onclick={openInstall}
      >
        {#snippet icon()}<UploadIcon size={14} />{/snippet}
        {acting === "install" ? i18n.t("rules.install.busy") : i18n.t("rules.install.action")}
      </Button>
    {/if}
  </div>

  {#if !needsRoot && installed === null && reason !== null}
    <p class="ri-reason">{reason}</p>
  {/if}

  <p class="ri-live" role="status" aria-live="polite">{announcement}</p>
</div>

<DestructiveConfirm
  open={confirmInstall}
  title={i18n.t("rules.install.confirmTitle")}
  confirmLabel={i18n.t("rules.install.action")}
  confirmVariant="primary"
  confirmDisabled={dest === null}
  onConfirm={doInstall}
  onCancel={() => (confirmInstall = false)}
>
  {#if scope === "project"}
    {i18n.t("rules.install.confirmProject", {
      rule: name,
      harness: label,
      project: projectName,
      dest: dest ?? "",
    })}
  {:else}
    {i18n.t("rules.install.confirmUser", { rule: name, harness: label, dest: dest ?? "" })}
  {/if}
</DestructiveConfirm>

<DestructiveConfirm
  open={confirmUninstall}
  title={i18n.t("rules.uninstall.confirmTitle", { rule: name, harness: label })}
  confirmLabel={i18n.t("rules.uninstall.action")}
  confirmVariant="danger"
  onConfirm={doUninstall}
  onCancel={() => (confirmUninstall = false)}
>
  <p>{i18n.t("rules.uninstall.confirm")}</p>
  {#if meta?.plugin}
    <p>{i18n.t("rules.uninstall.pluginKept", { plugin: meta.plugin.package })}</p>
  {/if}
</DestructiveConfirm>

<style>
  .ri { display: flex; flex-direction: column; gap: var(--space-2); }

  .ri-notice {
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    line-height: var(--lh-normal);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }

  .ri-actions { display: flex; align-items: center; gap: var(--space-3); flex-wrap: wrap; }
  /* text-secondary, not text-muted: this carries the disabled-install reason
     and must clear WCAG AA in dark mode, where the muted token measures 3.7:1. */
  .ri-state { font-size: var(--text-body-sm); color: var(--color-text-secondary); }
  .ri-dest {
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--color-text-muted);
    direction: ltr;
    unicode-bidi: isolate;
    word-break: break-all;
    min-width: 0;
  }
  .ri-reason { font-size: var(--text-caption); color: var(--color-text-secondary); }
  .ri-live { font-size: var(--text-caption); color: var(--color-text-secondary); min-height: 0; }
</style>
