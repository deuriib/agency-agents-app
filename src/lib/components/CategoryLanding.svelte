<script lang="ts">
  /**
   * CategoryLanding — the "browse by category" landing, generic over the catalog.
   *
   * Same model the Agents division landing uses (one row per category, with its
   * tinted icon and count, and an "all" row on top), without the agent-specific
   * bulk-select machinery: the Rules workspace needs the drill-in, not the
   * division deploy modal (issue #66).
   *
   * Ordering is decided by the caller and must be deterministic — the landing is
   * a navigation surface, and a list that reshuffles between loads is a list you
   * cannot learn.
   */
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import LayersIcon from "@lucide/svelte/icons/layers";
  import { resolveCategoryIcon } from "$lib/util/categoryIcon";
  import type { CategoryOption } from "./CategoryMenu.svelte";

  interface Props {
    /** Heading over the list, e.g. "Browse by category". */
    title: string;
    /** Label for the first row that shows everything. */
    allLabel: string;
    /** Total count behind the "all" row. */
    allCount: number;
    /** Formats "{count} rules" in the caller's locale (ICU pluralization). */
    countText: (count: number) => string;
    items: CategoryOption[];
    /** Called with the clicked category slug, or `null` for "all". */
    onPick: (slug: string | null) => void;
  }

  let { title, allLabel, allCount, countText, items, onPick }: Props = $props();
</script>

<div class="cl">
  <div class="cl-bar">
    <span class="lead"><LayersIcon size={14} /> {title}</span>
  </div>

  <ul class="rows" role="list">
    <li class="row">
      <button class="row-main" onclick={() => onPick(null)}>
        <span class="ic neutral"><LayersIcon size={18} /></span>
        <span class="text">
          <span class="name truncate">{allLabel}</span>
          <span class="meta">{countText(allCount)}</span>
        </span>
        <span class="chev" aria-hidden="true"><ChevronRight size={16} /></span>
      </button>
    </li>
    {#each items as item (item.slug)}
      {@const Icon = resolveCategoryIcon(item.icon)}
      <li class="row">
        <button class="row-main" onclick={() => onPick(item.slug)}>
          <span class="ic" style="color:{item.color}"><Icon size={18} /></span>
          <span class="text">
            <span class="name truncate">{item.label}</span>
            <span class="meta">{countText(item.count)}</span>
            {#if item.description}<span class="desc truncate">{item.description}</span>{/if}
          </span>
          <span class="chev" aria-hidden="true"><ChevronRight size={16} /></span>
        </button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .cl { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .cl-bar {
    display: flex; align-items: center; gap: var(--space-2);
    padding: var(--space-2) var(--space-1) var(--space-3);
  }
  .lead {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: var(--text-body-sm); color: var(--color-text-muted);
    font-weight: var(--fw-semibold); text-transform: uppercase; letter-spacing: 0.04em;
  }

  .rows { list-style: none; margin: 0; padding: 0; }
  .row { border-bottom: 1px solid var(--color-border); }
  .row-main {
    width: 100%; flex: 1; min-width: 0;
    display: flex; align-items: center; gap: var(--space-3);
    padding: var(--space-3) var(--space-2);
    text-align: left; background: transparent; cursor: pointer;
  }
  .row-main:hover { background: var(--color-surface-sunken); }
  .ic { flex: none; display: inline-flex; }
  .ic.neutral { color: var(--color-text-muted); }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .name { font-size: var(--text-body); font-weight: var(--fw-medium); color: var(--color-text-primary); }
  .meta { font-size: var(--text-caption); color: var(--color-text-muted); }
  .desc { font-size: var(--text-caption); color: var(--color-text-secondary); }
  .chev { flex: none; color: var(--color-text-muted); }
</style>
