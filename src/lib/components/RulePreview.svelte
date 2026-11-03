<script lang="ts">
  /**
   * RulePreview — the compiled output for one harness, plus the destination it
   * would be written to and a byte-level copy action.
   *
   * Two contracts shape this component (docs/rules-integration-ux.md §4):
   *  1. The bytes are the product. They render in a monospace `<pre>` exactly as
   *     returned — no markdown renderer, no highlighter, no trimming — and the
   *     copy action writes the returned string verbatim (trailing newline and
   *     all), so what the user pastes is what an install writes.
   *  2. A destination path is LTR in every locale. Without
   *     `direction: ltr` + `unicode-bidi: isolate` a path renders reversed under
   *     `fa` and the user copies the wrong thing.
   *
   * The harness switcher is a real tablist with roving tabindex; the options come
   * from the registry (`installableHarnesses()`), never a literal list, so a
   * harness this app cannot write never appears (spec §8).
   *
   * The tablist always renders, even for project scope with no folder picked:
   * the user must still be able to browse which harnesses exist. Only the
   * panel body becomes the "choose a project folder" state (there are no bytes
   * to show until a root resolves a destination).
   */
  import CopyIcon from "@lucide/svelte/icons/copy";
  import FolderOpenIcon from "@lucide/svelte/icons/folder-open";
  import { harnessLabel, installableHarnesses } from "$lib/data/ruleRegistry";
  import { i18n } from "$lib/stores/i18n.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { errorText, type RulePreviewEntry, type Scope } from "$lib/types";

  interface Props {
    /** The compiled entry for `harness`, already scope-checked by the parent. */
    entry: RulePreviewEntry | null;
    loading: boolean;
    error: string | null;
    harness: string | null;
    scope: Scope;
    /** The picked project root; `null` while project scope has none. */
    projectPath: string | null;
    onHarness: (harness: string) => void;
    onChooseProject: () => void;
  }

  let {
    entry,
    loading,
    error,
    harness,
    scope,
    projectPath,
    onHarness,
    onChooseProject,
  }: Props = $props();

  const harnesses = installableHarnesses();
  const needsRoot = $derived(scope === "project" && projectPath === null);
  /** UTF-8 byte length — rules carry em dashes and ✅/❌ glyphs, so `.length` lies. */
  const byteCount = $derived(entry === null ? 0 : new TextEncoder().encode(entry.bytes).length);

  let copied = $state(false);

  // The copy confirmation belongs to one entry: clear it whenever the shown
  // entry changes so the live region never reports a stale copy.
  $effect(() => {
    void entry?.harness;
    void scope;
    copied = false;
  });

  function selectHarness(id: string) {
    if (id === harness) return;
    copied = false;
    onHarness(id);
  }

  function isRTL(): boolean {
    return typeof document !== "undefined" && document.documentElement.dir === "rtl";
  }

  /** ArrowLeft/Right (mirrored in RTL) + Home/End, with focus following selection. */
  function onTabKey(event: KeyboardEvent) {
    if (harnesses.length === 0) return;
    const index = harnesses.findIndex((h) => h.id === harness);
    if (index < 0) return;

    const rtl = isRTL();
    let next: number;
    if (event.key === "ArrowRight") next = rtl ? index - 1 : index + 1;
    else if (event.key === "ArrowLeft") next = rtl ? index + 1 : index - 1;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = harnesses.length - 1;
    else return;

    event.preventDefault();
    const wrapped = (next + harnesses.length) % harnesses.length;
    const id = harnesses[wrapped].id;
    selectHarness(id);
    document.getElementById(`rp-tab-${id}`)?.focus();
  }

  async function copyBytes() {
    if (entry === null) return;
    try {
      // Verbatim, untrimmed, unwrapped — this IS the file content.
      await navigator.clipboard.writeText(entry.bytes);
      copied = true;
      toast.success(i18n.t("rules.preview.copied"));
    } catch (e) {
      toast.error(i18n.t("rules.preview.copyFailed"), errorText(e));
    }
  }
</script>

<!-- The tablist container is not a tab stop (the roving `tabindex` lives on
     the tabs); tabindex="-1" is what svelte-check's interactive-role rule
     wants to see on the role host.

     `aria-controls` references the ONE panel that exists, and only on the
     selected tab: rendering four hidden tabpanels would mean three empty panels
     (ux §4.4 — "an empty tabpanel is worse than no panel"), and pointing an
     inactive tab at `rp-panel-<id>` would be a dangling reference. Same
     single-panel shape as `Settings.svelte:140,151-153`. -->
<div
  class="rp-tabs"
  role="tablist"
  tabindex="-1"
  aria-label={i18n.t("rules.preview.tabsLabel")}
  onkeydown={onTabKey}
>
  {#each harnesses as h (h.id)}
    <button
      type="button"
      class="rp-tab"
      class:on={harness === h.id}
      role="tab"
      id={`rp-tab-${h.id}`}
      aria-selected={harness === h.id}
      aria-controls={harness === h.id ? "rp-panel" : undefined}
      tabindex={harness === h.id ? 0 : -1}
      onclick={() => selectHarness(h.id)}
    >
      {harnessLabel(h.id)}
    </button>
  {/each}
</div>

<div
  class="rp-panel"
  role="tabpanel"
  id="rp-panel"
  aria-labelledby={harness === null ? undefined : `rp-tab-${harness}`}
>
  {#if needsRoot}
    <div class="rp-notice">
      <p>{i18n.t("rules.scope.needsRoot")}</p>
      <button type="button" class="rp-link" onclick={onChooseProject}>
        <FolderOpenIcon size={14} />
        <span>{i18n.t("rules.scope.chooseProject")}</span>
      </button>
    </div>
  {:else if loading}
    <p class="rp-status">{i18n.t("rules.preview.loading")}</p>
  {:else if error !== null}
    <p class="rp-error">{i18n.t("rules.preview.failed", { error })}</p>
  {:else if entry !== null}
    <div class="rp-dest" id={`rp-dest-${entry.harness}`} dir="ltr">
      <span class="rp-dest-label">{i18n.t("rules.preview.destination")}</span>
      <code class="rp-dest-path">{entry.dest}</code>
    </div>
    <div class="rp-actions">
      <button type="button" class="rp-copy" onclick={copyBytes}>
        <CopyIcon size={14} />
        <span>{i18n.t("rules.preview.copy")}</span>
        <span class="rp-bytes-count">{i18n.t("rules.preview.bytes", { count: byteCount })}</span>
      </button>
      <span class="rp-live" role="status" aria-live="polite">
        {copied ? i18n.t("rules.preview.copied") : ""}
      </span>
    </div>
    <!-- One focus stop (the pre) so the bytes are keyboard-scrollable; the
         tabpanel itself does not take a stop — see ux §6 focus order. A
         scrollable region MUST be focusable (WCAG 2.1.1), so svelte-check's
         "noninteractive tabindex" heuristic is silenced here on purpose. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <pre class="rp-bytes" dir="ltr" tabindex="0" aria-describedby={`rp-dest-${entry.harness}`}>{entry.bytes}</pre>
  {/if}
</div>

<style>
  .rp-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    padding: 2px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
    align-self: flex-start;
  }
  .rp-tab {
    height: 26px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-secondary);
    font-size: var(--text-body-sm);
    white-space: nowrap;
    cursor: pointer;
  }
  .rp-tab:hover { color: var(--color-text-primary); background: var(--color-surface-raised); }
  .rp-tab.on {
    background: var(--color-surface-raised);
    color: var(--color-text-primary);
    font-weight: var(--fw-medium);
    box-shadow: var(--shadow-xs);
  }

  .rp-panel { display: flex; flex-direction: column; gap: var(--space-2); min-width: 0; }

  .rp-dest {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: var(--space-2);
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--color-text-muted);
    /* Paths are LTR even in an RTL UI — never bidi-reorder a destination. */
    direction: ltr;
    unicode-bidi: isolate;
    word-break: break-all;
  }
  .rp-dest-label { color: var(--color-text-muted); }
  .rp-dest-path { color: var(--color-text-primary); }

  .rp-actions { display: flex; align-items: center; gap: var(--space-3); flex-wrap: wrap; }
  .rp-copy {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 26px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    color: var(--color-text-primary);
    font-size: var(--text-body-sm);
    cursor: pointer;
  }
  .rp-copy:hover { background: var(--color-surface-sunken); border-color: var(--color-border-strong); }
  .rp-bytes-count {
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  /* Copy confirmation sits on `--color-surface-raised`, where the
     `-on-subtle` foregrounds are NOT a certified pair: those are certified
     against their own `-subtle` background. Secondary text is certified on a
     plain surface — measured 9.2:1 light / 7.5:1 dark. */
  .rp-live { font-size: var(--text-caption); color: var(--color-text-secondary); }

  .rp-bytes {
    margin: 0;
    padding: var(--space-3);
    max-height: 420px;
    overflow: auto;
    border-radius: var(--radius-md);
    background: var(--color-console-bg);
    color: var(--color-console-fg);
    font-family: var(--font-mono);
    font-size: var(--text-mono);
    line-height: var(--lh-mono);
    tab-size: 2;
    /* The bytes are the product: never wrap them. */
    white-space: pre;
    direction: ltr;
    unicode-bidi: isolate;
  }

  /* text-secondary: "Compiling…" is status text and must clear AA in dark mode. */
  .rp-status { font-size: var(--text-body-sm); color: var(--color-text-secondary); padding: var(--space-2) 0; }
  /* `danger-on-subtle` is certified only on `danger-subtle`, so the error gets
     that background instead of bare text on the raised surface
     (measured 6.5:1 light / 6.9:1 dark). */
  .rp-error {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-danger-subtle);
    color: var(--color-danger-on-subtle);
    font-size: var(--text-body-sm);
    line-height: var(--lh-normal);
  }

  .rp-notice {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px dashed var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }
  .rp-notice p { font-size: var(--text-body-sm); color: var(--color-text-secondary); }
  .rp-link {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 26px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    color: var(--color-text-primary);
    font-size: var(--text-body-sm);
    cursor: pointer;
  }
  .rp-link:hover { background: var(--color-surface-sunken); border-color: var(--color-border-strong); }
</style>
