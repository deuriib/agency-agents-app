<script lang="ts">
  /**
   * WorkspaceShell — the one list-plus-detail layout both workspaces use.
   *
   * The Agents workspace and the Rules workspace solve the same problem: browse
   * a catalog on the left, inspect one item in a resizable pane on the right.
   * They used to own two copies of this layout (`.ws/.list-pane/.lp-*` and
   * `.rv/.rv-main`), which meant two places to fix a bug and two different
   * resize behaviours for no product reason (issue #66).
   *
   * The shell owns the geometry and the shared chrome, never the content:
   *
   * - the flex row, the list pane, and the scroll container
   * - the resizable, persisted detail pane (only when something is selected)
   * - the close button, the Escape key, and the narrow-window overlay scrim
   *
   * Sections provide three snippets: `head` (the search/filter row), `list`
   * (the rows or a state), and `detail` (the selected item).
   */
  import XIcon from "@lucide/svelte/icons/x";
  import ResizeHandle from "./ResizeHandle.svelte";
  import type { Snippet } from "svelte";
  import { i18n } from "$lib/stores/i18n.svelte";
  import {
    ui,
    DETAIL_PANE_MIN_WIDTH,
    DETAIL_PANE_DEFAULT_WIDTH,
    clampDetailPaneWidth,
  } from "$lib/stores/ui.svelte";

  interface Props {
    /** True when the detail pane is open. */
    selected: boolean;
    /** Accessible name for the detail aside. */
    detailLabel: string;
    /** Called by the close button, the scrim, or Escape. */
    onClose: () => void;
    head: Snippet;
    list: Snippet;
    detail: Snippet;
  }

  let { selected, detailLabel, onClose, head, list, detail }: Props = $props();

  // Escape closes the pane, matching the Agents workspace's established
  // affordance. A dialog open on top owns Escape: closing both at once would
  // dismiss a confirmation the user was still reading.
  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape" || !selected) return;
    if (document.querySelector('[role="dialog"], [role="alertdialog"]') !== null) return;
    onClose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<section class="ws" class:sel={selected}>
  <div class="list-pane">
    <div class="lp-head">
      {@render head()}
    </div>
    <div class="lp-list">
      {@render list()}
    </div>
  </div>

  {#if selected}
    <!-- Grows the detail pane when dragged left; hidden under the overlay
         breakpoint, where the pane becomes a fixed sheet. -->
    <div class="ws-resize">
      <ResizeHandle
        width={ui.detailPaneWidth}
        min={DETAIL_PANE_MIN_WIDTH}
        max={900}
        defaultWidth={DETAIL_PANE_DEFAULT_WIDTH}
        direction="left"
        label={i18n.t("common.resizeDetailPane")}
        onChange={(w) => (ui.detailPaneWidth = clampDetailPaneWidth(w))}
        onCommit={(w) => ui.setDetailPaneWidth(w)}
      />
    </div>

    <aside class="detail-pane" style="width: {ui.detailPaneWidth}px" aria-label={detailLabel}>
      <div class="dp-bar">
        <button
          class="dp-close"
          onclick={onClose}
          aria-label={i18n.t("common.close")}
          title={i18n.t("common.close")}
        >
          <XIcon size={16} />
        </button>
      </div>
      <div class="dp-scroll">
        {@render detail()}
      </div>
    </aside>

    <!-- Narrow-window overlay scrim: clicking dismisses the overlaid pane. -->
    <button class="ws-scrim" aria-label={i18n.t("common.close")} onclick={onClose}></button>
  {/if}
</section>

<style>
  .ws { display: flex; height: 100%; min-height: 0; }

  /* ── List pane ── */
  .list-pane { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
  .lp-head {
    flex: none; padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border);
    display: flex; flex-direction: column; gap: var(--space-3);
  }
  .lp-list { flex: 1; overflow-y: auto; min-height: 0; padding: var(--space-2) var(--space-3); }

  /* ── Resize handle wrapper ── */
  .ws-resize { display: flex; flex: none; }

  /* ── Detail pane ── */
  .detail-pane {
    flex: none; max-width: 90vw;
    display: flex; flex-direction: column; min-height: 0;
    background: var(--color-surface-raised);
    border-left: 1px solid var(--color-border);
  }
  .dp-bar { flex: none; display: flex; justify-content: flex-end; padding: 6px 8px 0; }
  .dp-close {
    display: inline-flex; align-items: center; justify-content: center;
    width: 28px; height: 28px; border-radius: var(--radius-sm);
    color: var(--color-text-muted); background: transparent; cursor: pointer;
  }
  .dp-close:hover { background: var(--color-surface-sunken); color: var(--color-text-primary); }
  .dp-scroll { flex: 1; overflow-y: auto; min-height: 0; }

  /* Narrow-window overlay scrim — hidden by default, shown only under the
     breakpoint when a detail is open (see media query). */
  .ws-scrim { display: none; }

  @media (max-width: 860px) {
    .ws-resize { display: none; }
    .detail-pane {
      position: fixed; top: 36px; right: 0; bottom: 0; z-index: 41;
      width: min(var(--detail-w, 420px), 92vw) !important;
      box-shadow: var(--shadow-lg, -8px 0 24px rgba(0,0,0,0.18));
    }
    .ws:not(.sel) .detail-pane { display: none; }
    .ws.sel .ws-scrim {
      display: block; position: fixed; inset: 36px 0 0 0; z-index: 40;
      background: rgba(0,0,0,0.28); border: 0; cursor: default;
    }
  }
</style>
