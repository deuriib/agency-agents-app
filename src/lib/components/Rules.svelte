<script lang="ts">
  /**
   * Rules — the Rules workspace, rendered through the shared WorkspaceShell.
   *
   * It used to be a second, independent workspace: its own grid layout, its own
   * `<h1>`, its own filter controls, and categories reduced to a native select of
   * raw directory names. Now it is the Agents grammar applied to rules (issue
   * #66): the same list/detail geometry, the same resizable persisted detail
   * pane, the same category dropdown with icon + tint + count, a category landing
   * with an "all" entry, a category overview when one is selected, and a
   * segmented install-state lens with counts.
   *
   * Nine states are rendered distinctly, because collapsing them is how this
   * kind of screen lies to the user (docs/rules-integration-ux.md §3): loading,
   * load failure, not-synced, ships-no-rules, the category landing, partial
   * (invalid rules stay visible), filtered-to-zero, and per-rule installed /
   * not installed.
   */
  import { onMount } from "svelte";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import RefreshIcon from "@lucide/svelte/icons/refresh-cw";
  import ScrollTextIcon from "@lucide/svelte/icons/scroll-text";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import CategoryLanding from "./CategoryLanding.svelte";
  import CategoryMenu from "./CategoryMenu.svelte";
  import EmptyState from "./EmptyState.svelte";
  import Input from "./Input.svelte";
  import LoadingState from "./LoadingState.svelte";
  import Pill from "./Pill.svelte";
  import RuleDetail from "./RuleDetail.svelte";
  import WorkspaceShell from "./WorkspaceShell.svelte";
  import { harnessLabel, installableHarnesses } from "$lib/data/ruleRegistry";
  import { ruleCategoryMeta } from "$lib/data/ruleCategories";
  import { ruleCategoryTiles, ruleListState, type RuleLens } from "$lib/rules/derive";
  import { resolveCategoryIcon } from "$lib/util/categoryIcon";
  import { i18n } from "$lib/stores/i18n.svelte";
  import { rules } from "$lib/stores/rules.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { errorText, type RuleSummary } from "$lib/types";
  import type { MessageKey } from "$lib/i18n/messages";

  onMount(() => {
    // Re-reads status + list + ledger, so an install made in a previous session
    // is visible the moment this section opens (spec §10.6).
    void rules.load();
  });

  const harnesses = installableHarnesses();
  const visible = $derived(rules.filtered());
  const lensCounts = $derived(rules.lensCounts());
  const invalidCount = $derived(rules.invalidCount());
  const tiles = $derived(ruleCategoryTiles(rules.list, ruleCategoryMeta));

  /**
   * The landing is the default view, exactly like the Agents divisions landing:
   * no category, no search, no lens. Picking "All rules" (or narrowing anything)
   * switches to the flat list, so the "all" entry is a real destination rather
   * than a no-op.
   */
  let browsedAll = $state(false);
  const showLanding = $derived(
    !browsedAll && rules.categoryFilter === null && rules.query.trim() === "" && rules.lens === "all",
  );

  function pickCategory(slug: string | null) {
    browsedAll = slug === null;
    rules.setCategoryFilter(slug);
  }

  function clearFilters() {
    browsedAll = false;
    rules.clearFilters();
  }

  const listState = $derived(
    ruleListState({
      loaded: rules.loaded,
      loading: rules.loading,
      error: rules.error,
      listLength: rules.list.length,
      status: rules.status,
      visibleLength: visible.length,
    }),
  );

  const selected = $derived(
    rules.selectedSlug === null
      ? null
      : (rules.list.find((rule) => rule.slug === rules.selectedSlug) ?? null),
  );
  /** Only the body for the CURRENT selection — a late response is ignored. */
  const fullRule = $derived(
    rules.detail !== null && rules.detail.slug === rules.selectedSlug ? rules.detail : null,
  );

  const categoryLabel = $derived(
    rules.categoryFilter === null
      ? i18n.t("rules.all")
      : ruleCategoryMeta(rules.categoryFilter).label,
  );

  /**
   * The category overview: shown atop that category's list while browsing it
   * (not while searching), mirroring the Agents division banner. Rules have no
   * starter-prompt equivalent, so this carries metadata only.
   */
  const overview = $derived.by(() => {
    const slug = rules.categoryFilter;
    if (slug === null || rules.query.trim() !== "") return null;
    const tile = tiles.find((item) => item.slug === slug);
    if (tile === undefined) return null;
    return tile;
  });

  /** Lens definitions; `All` plus the two install states a rule can be in. */
  const LENSES: { id: RuleLens; key: MessageKey }[] = [
    { id: "all", key: "state.all" },
    { id: "installed", key: "rules.install.installed" },
    { id: "notInstalled", key: "rules.install.notInstalled" },
  ];

  let listEl: HTMLUListElement | undefined = $state();

  /** Accessible row name: name + category, then the state fragments. */
  function rowLabel(rule: RuleSummary): string {
    const parts = [i18n.t("rules.list.itemLabel", { name: rule.name, category: rule.category })];
    if (!rule.validation.valid) parts.push(i18n.t("rules.list.invalidLabel"));
    if (rules.installedHarnesses(rule.slug).size > 0) parts.push(i18n.t("rules.install.installed"));
    return parts.join(" · ");
  }

  /** ArrowUp/ArrowDown move the selection through the list; focus follows. */
  function onRowKey(event: KeyboardEvent, index: number) {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const next = event.key === "ArrowDown" ? index + 1 : index - 1;
    if (next < 0 || next >= visible.length) return;
    event.preventDefault();
    rules.select(visible[next].slug);
    listEl?.querySelectorAll<HTMLButtonElement>(".rv-row")[next]?.focus();
  }

  async function refresh() {
    try {
      await rules.refreshCorpus();
    } catch (e) {
      toast.error(i18n.t("common.actionFailed"), errorText(e));
    }
  }
</script>

<WorkspaceShell
  selected={selected !== null}
  detailLabel={selected?.name ?? i18n.t("rules.detail.emptyTitle")}
  onClose={() => rules.select(null)}
>
  {#snippet head()}
    <div class="rv-search-row">
      <CategoryMenu
        label={categoryLabel}
        selected={rules.categoryFilter}
        allCount={rules.list.length}
        allLabel={i18n.t("rules.all")}
        items={tiles}
        ariaLabel={i18n.t("rules.filter.category")}
        onPick={pickCategory}
      />
      <Input
        variant="search"
        value={rules.query}
        placeholder={i18n.t("rules.searchPlaceholder")}
        ariaLabel={i18n.t("rules.searchLabel")}
        onInput={(value) => rules.setQuery(value)}
      />
      {#if rules.filtersActive()}
        <button type="button" class="ghost" onclick={clearFilters}>
          {i18n.t("rules.filter.clear")}
        </button>
      {/if}
      <span class="rv-count" aria-hidden="true">
        {i18n.count(visible.length, "rules.count.one", "rules.count")}
      </span>
      <!-- Always available, like the Agents rescan: the empty states carry a CTA
           too, but a narrow filter can hide them. -->
      <button
        type="button"
        class="ghost icon"
        title={i18n.t("rules.empty.noRulesCta")}
        aria-label={i18n.t("rules.empty.noRulesCta")}
        disabled={rules.refreshing}
        onclick={refresh}
      >
        <RefreshIcon size={15} />
      </button>
    </div>

    {#if !showLanding && listState.kind === "rows" && rules.list.length > 0}
      <div class="seg" role="tablist" aria-label={i18n.t("rules.filter.installState")}>
        {#each LENSES as lens (lens.id)}
          <button
            type="button"
            class="seg-btn"
            class:on={rules.lens === lens.id}
            role="tab"
            aria-selected={rules.lens === lens.id}
            onclick={() => rules.setLens(lens.id)}
          >
            {i18n.t(lens.key)}
            <span class="seg-c">{lensCounts[lens.id]}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/snippet}

  {#snippet list()}
    {#if invalidCount > 0 && rules.error === null}
      <p class="rv-banner" role="status">
        {i18n.count(invalidCount, "rules.invalid.banner.one", "rules.invalid.banner.many")}
      </p>
    {/if}

    {#if listState.kind === "loading"}
      <div class="rv-state"><LoadingState rows={7} /></div>
    {:else if listState.kind === "error"}
      <div class="rv-state">
        <!-- Load failure is an `alert`, not a `status` (ux §6). -->
        <div class="rv-alert" role="alert">
          <span class="rv-alert-ico" aria-hidden="true"><TriangleAlertIcon size={22} /></span>
          <h2 class="rv-alert-title">{i18n.t("rules.loadFailedTitle")}</h2>
          <p class="rv-alert-body">{i18n.t("rules.loadFailed", { error: listState.message })}</p>
          <button type="button" class="rv-link" onclick={() => rules.load()}>
            {i18n.t("rules.retry")}
          </button>
        </div>
      </div>
    {:else if listState.kind === "notSynced" || listState.kind === "noRules"}
      <div class="rv-state">
        {#if listState.kind === "notSynced"}
          <EmptyState title={i18n.t("rules.empty.notSyncedTitle")}>
            {#snippet icon()}<DownloadIcon size={44} />{/snippet}
            {i18n.t("rules.empty.notSynced")}
            {#snippet cta()}
              <button type="button" class="rv-link" disabled={rules.refreshing} onclick={refresh}>
                {i18n.t("rules.empty.notSyncedCta")}
              </button>
            {/snippet}
          </EmptyState>
        {:else}
          <EmptyState title={i18n.t("rules.empty.noRulesTitle")}>
            {#snippet icon()}<ScrollTextIcon size={44} />{/snippet}
            {i18n.t("rules.empty.noRules")}
            {#snippet cta()}
              <button type="button" class="rv-link" disabled={rules.refreshing} onclick={refresh}>
                {i18n.t("rules.empty.noRulesCta")}
              </button>
            {/snippet}
          </EmptyState>
        {/if}
      </div>
    {:else if showLanding}
      <CategoryLanding
        title={i18n.t("rules.landing.title")}
        allLabel={i18n.t("rules.all")}
        allCount={rules.list.length}
        countText={(count) => i18n.count(count, "rules.count.one", "rules.count")}
        items={tiles}
        onPick={pickCategory}
      />
    {:else if listState.kind === "filteredToZero"}
      <div class="rv-filtered">
        <p>{i18n.t("rules.filtered.none")}</p>
        <button type="button" class="rv-link" onclick={clearFilters}>
          {i18n.t("rules.filtered.clear")}
        </button>
      </div>
    {:else}
      {#if overview}
        {@const Icon = resolveCategoryIcon(overview.icon)}
        <div class="dov">
          <span class="dov-ic" style="color:{overview.color}"><Icon size={18} /></span>
          <div class="dov-id">
            <span class="dov-name">{overview.label}</span>
            <span class="dov-sub">
              {i18n.count(overview.count, "rules.count.one", "rules.count")}
            </span>
          </div>
          {#if overview.description}<p class="dov-desc">{overview.description}</p>{/if}
        </div>
      {/if}
      <ul class="rv-list" role="list" bind:this={listEl}>
        {#each visible as rule, index (rule.slug)}
          {@const installed = rules.installedHarnesses(rule.slug)}
          {@const meta = ruleCategoryMeta(rule.category)}
          {@const CategoryIcon = resolveCategoryIcon(meta.icon)}
          <li>
            <button
              type="button"
              class="rv-row"
              aria-current={rules.selectedSlug === rule.slug ? "true" : undefined}
              aria-label={rowLabel(rule)}
              onclick={() => rules.select(rule.slug)}
              onkeydown={(event) => onRowKey(event, index)}
            >
              <span class="rv-row-ic" aria-hidden="true" style="color:{meta.color}">
                <CategoryIcon size={16} />
              </span>
              <span class="rv-row-text">
                <span class="rv-row-name">{rule.name}</span>
                <span class="rv-row-meta">{rule.description}</span>
              </span>
              {#if !rule.validation.valid}
                <span class="rv-badge" title={i18n.t("rules.invalid.badgeTitle")}>
                  <Pill tone="danger">{i18n.t("rules.invalid.badge")}</Pill>
                </span>
              {/if}
              <span class="rv-dots" aria-hidden="true">
                {#each harnesses as harness (harness.id)}
                  <span
                    class="rv-dot"
                    class:on={installed.has(harness.id)}
                    title={harnessLabel(harness.id)}
                  ></span>
                {/each}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/snippet}

  {#snippet detail()}
    <RuleDetail
      summary={selected}
      rule={fullRule}
      loading={rules.detailLoading}
      error={rules.detailError}
    />
  {/snippet}
</WorkspaceShell>

<style>
  /* Only the rules-specific content lives here: the pane geometry, the resize
     handle, the close chrome and the scrim belong to WorkspaceShell. */

  .rv-search-row { display: flex; align-items: center; gap: var(--space-2); }
  .rv-search-row :global(.wrap) { flex: 1; min-width: 0; }

  .ghost {
    display: inline-flex; align-items: center; gap: 6px; flex: none;
    height: 32px; padding: 0 var(--space-3);
    border: 1px solid var(--color-border); border-radius: var(--radius-md);
    background: transparent; color: var(--color-text-secondary);
    font-size: var(--text-body-sm); cursor: pointer; white-space: nowrap;
  }
  .ghost:hover:not(:disabled) { color: var(--color-text-primary); background: var(--color-surface-sunken); }
  .ghost:disabled { opacity: 0.6; cursor: default; }
  .ghost.icon { padding: 0; width: 32px; justify-content: center; }

  .rv-count {
    flex: none;
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  /* ── Install-state lens (same segmented control as the Agents workspace) ── */
  .seg {
    display: flex; align-items: center; gap: 2px; flex-wrap: wrap; min-width: 0;
    padding: 2px;
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-border); border-radius: var(--radius-md);
  }
  .seg-btn {
    display: inline-flex; align-items: center; gap: 6px;
    height: 26px; padding: 0 10px; border-radius: var(--radius-sm);
    background: transparent; color: var(--color-text-secondary);
    font-size: var(--text-body-sm); cursor: pointer; white-space: nowrap;
  }
  .seg-btn:hover { color: var(--color-text-primary); }
  .seg-btn.on { background: var(--color-surface-raised); color: var(--color-text-primary); box-shadow: var(--shadow-sm, 0 1px 2px rgba(0,0,0,0.08)); }
  .seg-c { color: var(--color-text-muted); font-variant-numeric: tabular-nums; }
  .seg-btn.on .seg-c { color: var(--color-text-secondary); }

  /* Partial state: invalid rules stay listed, and the banner says how many
     cannot be installed. `--color-warning-strong` is the AA-safe text tone. */
  .rv-banner {
    margin: 0 0 var(--space-2);
    font-size: var(--text-caption);
    color: var(--color-warning-strong);
  }

  /* ── Category overview (the Agents division banner's counterpart) ── */
  .dov {
    margin: var(--space-1) var(--space-1) var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--color-border); border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
    display: flex; align-items: center; gap: var(--space-3); flex-wrap: wrap;
  }
  .dov-ic { flex: none; display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; border-radius: var(--radius-md); background: var(--color-surface-sunken); }
  .dov-id { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .dov-name { font-size: var(--text-body); font-weight: var(--fw-semibold); color: var(--color-text-primary); }
  .dov-sub { font-size: var(--text-caption); color: var(--color-text-muted); }
  .dov-desc { flex-basis: 100%; margin: 0; font-size: var(--text-caption); color: var(--color-text-secondary); }

  /* ── Rows ── */
  .rv-list { display: flex; flex-direction: column; gap: 1px; }
  .rv-row {
    width: 100%; display: flex; align-items: center; gap: var(--space-3);
    padding: var(--space-2); border-radius: var(--radius-md);
    background: transparent; color: inherit; text-align: left; cursor: pointer;
  }
  .rv-row:hover { background: var(--color-surface-sunken); }
  .rv-row[aria-current="true"] { background: var(--color-brand-subtle); }
  .rv-row-ic { flex: none; display: inline-flex; }
  .rv-row-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .rv-row-name { font-size: var(--text-body-sm); font-weight: var(--fw-medium); color: var(--color-text-primary); }
  .rv-row-meta {
    font-size: var(--text-caption); color: var(--color-text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .rv-badge { flex: none; display: inline-flex; }
  .rv-dots { display: inline-flex; align-items: center; gap: 3px; flex: none; }
  .rv-dot { width: 7px; height: 7px; border-radius: 999px; background: var(--color-text-muted); opacity: 0.35; }
  .rv-dot.on { background: var(--color-success); opacity: 1; }

  /* ── States ── */
  .rv-state { padding: var(--space-3); }
  .rv-alert {
    display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-2);
    padding: var(--space-4);
    border: 1px solid var(--color-border); border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
  }
  .rv-alert-ico { color: var(--color-danger); display: inline-flex; }
  .rv-alert-title { font-size: var(--text-body); font-weight: var(--fw-semibold); }
  .rv-alert-body { font-size: var(--text-body-sm); color: var(--color-text-secondary); }
  .rv-link {
    background: transparent; color: var(--color-brand);
    font-size: var(--text-body-sm); font-weight: var(--fw-medium); cursor: pointer;
  }
  .rv-link:disabled { color: var(--color-text-muted); cursor: default; }
  .rv-filtered { display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-2); padding: var(--space-4); }
</style>
