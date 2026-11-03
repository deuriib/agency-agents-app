<script lang="ts">
  /**
   * CategoryMenu — the category dropdown both workspaces use.
   *
   * Agents and Rules both present their catalog grouped by category, and they
   * used to do it with two different controls (an OS-style menu and a native
   * `<select>` of raw directory names). One component now, so the two screens
   * agree on the metadata shown per option — icon, tint, label, count — and on
   * the keyboard/dismiss behaviour (issue #66).
   *
   * The menu is a `role="menu"` of `menuitem` buttons; opening moves nothing, so
   * the trigger keeps focus and Escape returns it. Clicking outside closes.
   */
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import LayersIcon from "@lucide/svelte/icons/layers";
  import { resolveCategoryIcon } from "$lib/util/categoryIcon";

  export interface CategoryOption {
    slug: string;
    label: string;
    icon: string;
    color: string;
    count: number;
    /** Optional one-line explanation, rendered by the landing when present. */
    description?: string;
  }

  interface Props {
    /** Label shown on the trigger — the active category or the "all" label. */
    label: string;
    /** Selected category, or `null` for "all". */
    selected: string | null;
    /** The "all" entry's count. */
    allCount: number;
    /** Accessible name for the "all" entry, e.g. "All divisions" / "All rules". */
    allLabel: string;
    items: CategoryOption[];
    ariaLabel: string;
    open?: boolean;
    onPick: (slug: string | null) => void;
  }

  let { label, selected, allCount, allLabel, items, ariaLabel, open = $bindable(false), onPick }: Props =
    $props();

  let btn = $state<HTMLElement>();
  let menu = $state<HTMLElement>();

  function onDocClick(event: MouseEvent) {
    const target = event.target as Node | null;
    if (!target || !open) return;
    if (btn?.contains(target) || menu?.contains(target)) return;
    open = false;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && open) {
      open = false;
      btn?.querySelector("button")?.focus();
    }
  }

  function pick(slug: string | null) {
    open = false;
    onPick(slug);
  }
</script>

<svelte:document onclick={onDocClick} />
<svelte:window onkeydown={onKeydown} />

<div class="cat-wrap">
  <button
    class="ghost cat-btn"
    bind:this={btn}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label={ariaLabel}
    onclick={() => (open = !open)}
  >
    <span class="truncate">{label}</span><ChevronDown size={13} />
  </button>
  {#if open}
    <div class="cat-menu" role="menu" aria-label={ariaLabel} bind:this={menu}>
      <button
        class="cat-opt"
        role="menuitem"
        class:on={selected === null}
        onclick={() => pick(null)}
      >
        <LayersIcon size={14} /><span class="truncate">{allLabel}</span><span class="cat-c"
          >{allCount}</span
        >
      </button>
      {#each items as item (item.slug)}
        {@const Icon = resolveCategoryIcon(item.icon)}
        <button
          class="cat-opt"
          role="menuitem"
          class:on={selected === item.slug}
          onclick={() => pick(item.slug)}
        >
          <span class="cat-ic" style="color:{item.color}"><Icon size={14} /></span>
          <span class="truncate">{item.label}</span>
          <span class="cat-c">{item.count}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .ghost {
    display: inline-flex; align-items: center; gap: 6px; flex: none;
    height: 32px; padding: 0 var(--space-3);
    border: 1px solid var(--color-border); border-radius: var(--radius-md);
    background: transparent; color: var(--color-text-secondary);
    font-size: var(--text-body-sm); cursor: pointer;
  }
  .ghost:hover:not(:disabled) { color: var(--color-text-primary); background: var(--color-surface-sunken); }

  .cat-wrap { position: relative; }
  .cat-btn { max-width: 180px; }
  .cat-menu {
    position: absolute; top: calc(100% + 4px); left: 0; z-index: 30;
    min-width: 220px; max-height: 320px; overflow-y: auto; padding: 4px;
    background: var(--color-surface-raised); border: 1px solid var(--color-border);
    border-radius: var(--radius-md); box-shadow: var(--shadow-lg);
    display: flex; flex-direction: column; gap: 1px;
  }
  .cat-opt {
    display: flex; align-items: center; gap: var(--space-2);
    padding: 6px 8px; border-radius: var(--radius-sm);
    background: transparent; color: var(--color-text-primary);
    font-size: var(--text-body-sm); text-align: left; cursor: pointer;
  }
  .cat-opt:hover { background: var(--color-surface-sunken); }
  .cat-opt.on { color: var(--color-brand); }
  .cat-opt .truncate { flex: 1; min-width: 0; }
  /* Category icon tinted with its own accent; dim to brand when the row is the
     active selection so the "on" state stays legible. */
  .cat-ic { display: inline-flex; flex: none; }
  .cat-opt.on .cat-ic { color: var(--color-brand) !important; }
  .cat-c { font-size: var(--text-caption); color: var(--color-text-muted); }
</style>
