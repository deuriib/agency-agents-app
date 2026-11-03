<script lang="ts">
  /**
   * Rules — the Rules workspace: a searchable, filterable catalog of the rules in
   * the managed `agency-rules` corpus on the left, the selected rule's detail on
   * the right.
   *
   * Seven states are rendered distinctly, because collapsing them is how this
   * kind of screen lies to the user (docs/rules-integration-ux.md §3):
   * loading, load failure (message + retry, never a fake empty state), empty
   * because the corpus is **not synced**, empty because the corpus **ships no
   * rules** (different sentence, icon, and CTA), partial (invalid rules stay
   * visible with a reason), filtered-to-zero (the corpus is fine — the query is
   * the problem), and per-rule installed / not installed.
   *
   * The two panes are a CSS grid split. A draggable divider is M2 polish: the
   * existing one is driven by `ui.detailPaneWidth`, which is agent-specific nav
   * state (ux §2.2).
   */
  import { onMount } from "svelte";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import ScrollTextIcon from "@lucide/svelte/icons/scroll-text";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import EmptyState from "./EmptyState.svelte";
  import Input from "./Input.svelte";
  import LoadingState from "./LoadingState.svelte";
  import Pill from "./Pill.svelte";
  import RuleDetail from "./RuleDetail.svelte";
  import { harnessLabel, installableHarnesses } from "$lib/data/ruleRegistry";
  import { i18n } from "$lib/stores/i18n.svelte";
  import { rules } from "$lib/stores/rules.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { errorText, type RuleSummary } from "$lib/types";

  onMount(() => {
    // Re-reads status + list + ledger, so an install made in a previous session
    // is visible the moment this section opens (spec §10.6).
    void rules.load();
  });

  const harnesses = installableHarnesses();
  const visible = $derived(rules.filtered());
  const categories = $derived(rules.categories());
  const invalidCount = $derived(rules.invalidCount());
  const selected = $derived(
    rules.selectedSlug === null
      ? null
      : (rules.list.find((rule) => rule.slug === rules.selectedSlug) ?? null),
  );
  /** Only the body for the CURRENT selection — a late response is ignored. */
  const detail = $derived(
    rules.detail !== null && rules.detail.slug === rules.selectedSlug ? rules.detail : null,
  );

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

<section class="rv">
  <header class="rv-head" data-tauri-drag-region>
    <div class="rv-titles" data-tauri-drag-region="false">
      <h1 class="rv-title">{i18n.t("nav.rules")}</h1>
      <p class="rv-sub">{i18n.t("rules.subtitle")}</p>
    </div>
  </header>

  <div class="rv-toolbar">
    <Input
      variant="search"
      value={rules.query}
      placeholder={i18n.t("rules.searchPlaceholder")}
      ariaLabel={i18n.t("rules.searchLabel")}
      onInput={(value) => rules.setQuery(value)}
    />

    <label class="sr-only" for="rv-filter-harness">{i18n.t("rules.filter.harness")}</label>
    <select
      id="rv-filter-harness"
      class="rv-filter"
      value={rules.harnessFilter ?? ""}
      onchange={(e) => rules.setHarnessFilter(e.currentTarget.value || null)}
    >
      <option value="">{i18n.t("rules.filter.harnessAll")}</option>
      {#each harnesses as harness (harness.id)}
        <option value={harness.id}>{harnessLabel(harness.id)}</option>
      {/each}
    </select>

    <label class="sr-only" for="rv-filter-category">{i18n.t("rules.filter.category")}</label>
    <select
      id="rv-filter-category"
      class="rv-filter"
      value={rules.categoryFilter ?? ""}
      onchange={(e) => rules.setCategoryFilter(e.currentTarget.value || null)}
    >
      <option value="">{i18n.t("rules.filter.categoryAll")}</option>
      {#each categories as category (category)}
        <option value={category}>{category}</option>
      {/each}
    </select>

    {#if rules.filtersActive()}
      <button type="button" class="rv-clear" onclick={() => rules.clearFilters()}>
        {i18n.t("rules.filter.clear")}
      </button>
    {/if}

    <span class="rv-count" aria-hidden="true">
      {i18n.t("rules.count", { count: visible.length })}
    </span>
  </div>

  {#if invalidCount > 0 && rules.error === null}
    <p class="rv-banner" role="status">
      {i18n.count(invalidCount, "rules.invalid.banner.one", "rules.invalid.banner.many")}
    </p>
  {/if}

  <div class="rv-main">
    <div class="rv-list-pane" role="region" aria-label={i18n.t("rules.list.regionLabel")}>
      {#if !rules.loaded || rules.loading}
        <div class="rv-state"><LoadingState rows={7} /></div>
      {:else if rules.error !== null}
        <div class="rv-state">
          <!-- Load failure is an `alert`, not a `status` (ux §6). `EmptyState`
               hardcodes `role="status"`, and loosening that for every caller
               would change unrelated screens — so the alert variant is rendered
               inline here. -->
          <div class="rv-alert" role="alert">
            <span class="rv-alert-ico" aria-hidden="true"><TriangleAlertIcon size={22} /></span>
            <h2 class="rv-alert-title">{i18n.t("rules.loadFailedTitle")}</h2>
            <p class="rv-alert-body">{i18n.t("rules.loadFailed", { error: rules.error })}</p>
            <button type="button" class="rv-link" onclick={() => rules.load()}>
              {i18n.t("rules.retry")}
            </button>
          </div>
        </div>
      {:else if rules.list.length === 0}
        <div class="rv-state">
          {#if rules.status?.emptyReason === "notSynced"}
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
      {:else if visible.length === 0}
        <div class="rv-filtered">
          <p>{i18n.t("rules.filtered.none")}</p>
          <button type="button" class="rv-link" onclick={() => rules.clearFilters()}>
            {i18n.t("rules.filtered.clear")}
          </button>
        </div>
      {:else}
        <ul class="rv-list" role="list" bind:this={listEl}>
          {#each visible as rule, index (rule.slug)}
            {@const installed = rules.installedHarnesses(rule.slug)}
            <li>
              <button
                type="button"
                class="rv-row"
                aria-current={rules.selectedSlug === rule.slug ? "true" : undefined}
                aria-label={rowLabel(rule)}
                onclick={() => rules.select(rule.slug)}
                onkeydown={(event) => onRowKey(event, index)}
              >
                <span class="rv-row-text">
                  <span class="rv-row-name">{rule.name}</span>
                  <span class="rv-row-meta">{rule.description}</span>
                </span>
                <Pill tone="neutral">{rule.category}</Pill>
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
    </div>

    <aside class="rv-detail">
      <RuleDetail
        summary={selected}
        rule={detail}
        loading={rules.detailLoading}
        error={rules.detailError}
      />
    </aside>
  </div>
</section>

<style>
  .rv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .rv-head {
    flex: none;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }
  .rv-title {
    font-size: var(--text-h2);
    font-weight: var(--fw-semibold);
    color: var(--color-text-primary);
  }
  .rv-sub {
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    margin-top: 1px;
  }

  .rv-toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4) var(--space-2);
  }
  .rv-toolbar :global(.wrap) { flex: 1; min-width: 0; }
  .rv-filter {
    height: 30px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    color: var(--color-text-secondary);
    font-size: var(--text-body-sm);
    max-width: 200px;
    cursor: pointer;
  }
  .rv-clear {
    flex: none;
    height: 30px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text-secondary);
    font-size: var(--text-body-sm);
    cursor: pointer;
    white-space: nowrap;
  }
  .rv-clear:hover { background: var(--color-surface-sunken); color: var(--color-text-primary); }
  .rv-count {
    flex: none;
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  /* Partial state: invalid rules stay listed, and the banner says how many
     cannot be installed. `--color-warning-strong` is the AA-safe text tone. */
  .rv-banner {
    flex: none;
    margin: 0 var(--space-4) var(--space-1);
    font-size: var(--text-caption);
    color: var(--color-warning-strong);
  }

  .rv-main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(280px, 40%) minmax(360px, 1fr);
    gap: var(--space-3);
    padding: var(--space-3);
  }
  .rv-list-pane { min-width: 0; min-height: 0; display: flex; flex-direction: column; }
  .rv-state {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  /* Inline alert variant of the load-failure state (role="alert", ux §6). Same
     centering as `EmptyState` so the pane does not shift between states. */
  .rv-alert {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: var(--space-3);
    max-width: 360px;
    margin: auto;
    padding: var(--space-6);
  }
  .rv-alert-ico { color: var(--color-text-muted); display: flex; }
  .rv-alert-title {
    font-size: var(--text-h2);
    font-weight: var(--fw-semibold);
    color: var(--color-text-primary);
  }
  .rv-alert-body {
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    line-height: var(--lh-normal);
  }

  .rv-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .rv-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    text-align: start;
    cursor: pointer;
    color: var(--color-text-primary);
  }
  .rv-row:hover { background: var(--color-surface-sunken); }
  .rv-row[aria-current="true"] {
    background: var(--color-brand-subtle);
    border-color: var(--color-border);
  }
  .rv-row-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .rv-row-name {
    font-size: var(--text-body-sm);
    font-weight: var(--fw-medium);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rv-row-meta {
    font-size: var(--text-caption);
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rv-badge { flex: none; display: inline-flex; }

  .rv-dots { flex: none; display: inline-flex; align-items: center; gap: 3px; }
  .rv-dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-full);
    background: var(--color-border-strong);
  }
  .rv-dot.on { background: var(--color-success); }

  /* Filtered-to-zero lives inside the list pane: the corpus is fine, the query
     is the problem — so this is not a full-pane empty state. */
  .rv-filtered {
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3);
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
  }

  .rv-link {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 2px 4px;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-link);
    font-size: var(--text-body-sm);
    cursor: pointer;
  }
  .rv-link:hover:not(:disabled) { text-decoration: underline; }
  .rv-link:disabled { opacity: 0.5; cursor: default; }

  .rv-detail {
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
  }
</style>
