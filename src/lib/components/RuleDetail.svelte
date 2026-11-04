<script lang="ts">
  /**
   * RuleDetail — the right-hand pane for one rule: identity, frontmatter
   * summary, the neutral markdown body (as text — no renderer is in scope), the
   * validation verdict, the compiled preview, and the install controls.
   *
   * This component owns the ONE scope control in the app (ux §4.1). The scope
   * and the project root drive both the preview and the install action, because
   * two controls would let a user preview user-scope bytes while installing into
   * a project. Project scope is a directory picker result held in local state —
   * never a Projects-registry entry (TASK 21).
   *
   * The header also opens the shared destinations × harnesses grid (issue #92).
   * That is an ADDITIONAL path: this pane's scope control and
   * `RuleInstallControls` stay where they are as the precise, per-triple path.
   */
  import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
  import FolderOpenIcon from "@lucide/svelte/icons/folder-open";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import UploadIcon from "@lucide/svelte/icons/upload";
  import Button from "./Button.svelte";
  import DestructiveConfirm from "./DestructiveConfirm.svelte";
  import EmptyState from "./EmptyState.svelte";
  import InstallModal from "./InstallModal.svelte";
  import LoadingState from "./LoadingState.svelte";
  import Pill from "./Pill.svelte";
  import RulePreview from "./RulePreview.svelte";
  import RuleInstallControls from "./RuleInstallControls.svelte";
  import { harnessLabel, installableHarnesses, ruleHarness } from "$lib/data/ruleRegistry";
  import { i18n } from "$lib/stores/i18n.svelte";
  import { rules } from "$lib/stores/rules.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import {
    errorText,
    type Rule,
    type RuleGlobs,
    type RuleInstall,
    type RulePreviewEntry,
    type RuleSummary,
    type Scope,
  } from "$lib/types";

  interface Props {
    /** List row for the selected rule — present before the body arrives. */
    summary: RuleSummary | null;
    /** Full rule (body + hashes) once `rules_get` resolves. */
    rule: Rule | null;
    loading: boolean;
    error: string | null;
  }

  let { summary, rule, loading, error }: Props = $props();

  /** Everything the neutral pane renders lives on both shapes. */
  const view = $derived<Rule | RuleSummary | null>(rule ?? summary);
  const ruleSlug = $derived(view?.slug ?? null);
  const isValid = $derived(view?.validation.valid ?? false);
  const validationErrors = $derived(view?.validation.errors ?? []);

  const harnesses = installableHarnesses();
  let chosenHarness = $state<string | null>(null);
  const activeHarness = $derived<string | null>(chosenHarness ?? harnesses[0]?.id ?? null);
  const harnessMeta = $derived(activeHarness === null ? null : ruleHarness(activeHarness));

  let scope = $state<Scope>("user");
  let projectPath = $state<string | null>(null);

  /**
   * Every ledger row for this rule, whatever harness/scope/root it was written
   * with. The project root is otherwise lost the moment the picker state
   * changes, so this list is the only place a project install can be found
   * again to uninstall it (TASK 21).
   */
  const installs = $derived(ruleSlug === null ? [] : rules.installsFor(ruleSlug));

  /** Row awaiting uninstall confirmation. */
  let uninstallRow: RuleInstall | null = $state(null);

  /** The bulk grid (#92) is opened from the header and owned right here. */
  let installOpen = $state(false);

  /**
   * Accessible name of the grid entry point: the action plus the rule name.
   * An invalid rule can never write files, so the disabled control states WHY
   * (disabled-without-a-reason is a defect) — in `title` and in the name.
   */
  const installLabel = $derived(
    view === null ? "" : i18n.t("rules.install.gridTitle", { rule: view.name }),
  );
  const installReason = $derived(isValid ? null : i18n.t("rules.install.reasonInvalid"));
  const installAria = $derived(
    installReason === null ? installLabel : `${installLabel} — ${installReason}`,
  );

  /** Only the scopes the active harness actually declares (derived, never literal). */
  const availableScopes = $derived.by((): Scope[] => {
    const meta = harnessMeta;
    if (meta === null) return [];
    const scopes: Scope[] = [];
    if (meta.dest.user !== undefined) scopes.push("user");
    if (meta.dest.project !== undefined) scopes.push("project");
    return scopes;
  });

  const activeScope = $derived<Scope>(
    availableScopes.includes(scope) ? scope : (availableScopes[0] ?? "user"),
  );

  /** Compiled entry for the shown harness, valid only for the shown scope. */
  const entry = $derived.by((): RulePreviewEntry | null => {
    if (ruleSlug === null || activeHarness === null) return null;
    return rules.previewEntry(ruleSlug, activeHarness, activeScope, projectPath);
  });

  const hasFrontmatter = $derived(
    view !== null &&
      (view.globs !== undefined ||
        view.alwaysApply !== undefined ||
        view.trigger !== undefined ||
        view.match !== undefined ||
        view.tags.length > 0 ||
        view.tools.length > 0),
  );

  // Recompile whenever the rule, scope, or project root changes — one response
  // carries every harness, so switching harness is a cache hit, never a refetch
  // (TASK 20). An invalid rule has no bytes to compile (the backend refuses it),
  // so it never reaches the preview at all.
  $effect(() => {
    if (ruleSlug === null || !isValid) return;
    const root = activeScope === "project" ? projectPath : null;
    if (activeScope === "project" && root === null) return;
    void rules.preview(ruleSlug, root);
  });

  function globsText(globs: RuleGlobs): string {
    return Array.isArray(globs) ? globs.join(", ") : globs;
  }

  function isRTL(): boolean {
    return typeof document !== "undefined" && document.documentElement.dir === "rtl";
  }

  function setScope(next: Scope) {
    if (next === activeScope) return;
    scope = next;
  }

  /** Roving tabindex for the scope radiogroup; arrows mirror in RTL. */
  function onScopeKey(event: KeyboardEvent) {
    if (availableScopes.length < 2) return;
    const index = availableScopes.indexOf(activeScope);
    if (index < 0) return;

    const rtl = isRTL();
    let next: number;
    if (event.key === "ArrowRight") next = rtl ? index - 1 : index + 1;
    else if (event.key === "ArrowLeft") next = rtl ? index + 1 : index - 1;
    else return;

    event.preventDefault();
    const wrapped = (next + availableScopes.length) % availableScopes.length;
    const target = availableScopes[wrapped];
    setScope(target);
    document.getElementById(`rd-scope-${target}`)?.focus();
  }

  async function chooseProject() {
    try {
      const picked = await openFolderDialog({
        directory: true,
        multiple: false,
        title: i18n.t("rules.scope.chooseProjectTitle"),
      });
      if (typeof picked === "string") projectPath = picked;
    } catch (e) {
      toast.error(i18n.t("common.actionFailed"), errorText(e));
    }
  }

  // No listeners or timers are registered here, and the `$effect` above is
  // disposed with the component — nothing else needs tearing down.
  function retry() {
    if (ruleSlug === null) return;
    void rules.loadBody(ruleSlug);
  }

  /** Identity of one ledger row inside this rule — unique per harness+scope+root. */
  function rowKey(row: RuleInstall): string {
    return `${row.tool}\u0000${row.scope}\u0000${row.projectPath ?? ""}`;
  }

  /** "User", or the project root the row was written into. */
  function installScopeLabel(row: RuleInstall): string {
    if (row.scope === "user") return i18n.t("rules.scope.user");
    return row.projectPath ?? i18n.t("rules.scope.projectRootNone");
  }

  /** Uninstall one ledger row (harness + scope + root), then re-read the ledger. */
  async function doUninstallRow() {
    const row = uninstallRow;
    uninstallRow = null;
    if (row === null) return;
    try {
      await rules.uninstall(row.slug, row.tool, row.scope, row.projectPath);
      toast.success(i18n.t("rules.uninstall.success"));
    } catch (e) {
      toast.error(i18n.t("rules.uninstall.failed", { error: errorText(e) }));
    }
  }
</script>

{#if view === null}
  {#if loading}
    <LoadingState rows={4} />
  {:else}
    <EmptyState
      title={i18n.t("rules.detail.emptyTitle")}
      body={i18n.t("rules.detail.empty")}
    />
  {/if}
{:else}
  <div class="rd">
    <header class="rd-head">
      <div class="rd-ident">
        <h2 class="rd-title">{view.name}</h2>
        <Pill tone="neutral">{view.category}</Pill>
        <span class="rd-grid-open">
          <Button
            variant="secondary"
            disabled={!isValid}
            title={installReason ?? undefined}
            ariaLabel={installAria}
            onclick={() => (installOpen = true)}
          >
            {#snippet icon()}<UploadIcon size={14} />{/snippet}
            {i18n.t("rules.install.gridOpen")}
          </Button>
        </span>
      </div>
      <p class="rd-desc">{view.description}</p>
    </header>

    {#if error !== null}
      <div class="rd-load-error" role="alert">
        <p class="rd-load-error-title">{i18n.t("rules.loadFailedTitle")}</p>
        <p class="rd-load-error-body">{i18n.t("rules.loadFailed", { error })}</p>
        <button type="button" class="rd-link" onclick={retry}>{i18n.t("rules.retry")}</button>
      </div>
    {/if}

    {#if hasFrontmatter}
      <section class="rd-section">
        <h3 class="rd-h3">{i18n.t("rules.detail.metadata")}</h3>
        <dl class="rd-fm">
          {#if view.globs !== undefined}
            <dt>{i18n.t("rules.frontmatter.globs")}</dt>
            <dd>{globsText(view.globs)}</dd>
          {/if}
          {#if view.alwaysApply !== undefined}
            <dt>{i18n.t("rules.frontmatter.alwaysApply")}</dt>
            <dd>{String(view.alwaysApply)}</dd>
          {/if}
          {#if view.trigger !== undefined}
            <dt>{i18n.t("rules.frontmatter.trigger")}</dt>
            <dd>{view.trigger}</dd>
          {/if}
          {#if view.match !== undefined}
            <dt>{i18n.t("rules.frontmatter.match")}</dt>
            <dd>{view.match}</dd>
          {/if}
          {#if view.tags.length > 0}
            <dt>{i18n.t("rules.frontmatter.tags")}</dt>
            <dd>{view.tags.join(", ")}</dd>
          {/if}
          {#if view.tools.length > 0}
            <dt>{i18n.t("rules.frontmatter.tools")}</dt>
            <dd>{view.tools.join(", ")}</dd>
          {/if}
        </dl>
      </section>
    {/if}

    <section class="rd-section">
      <h3 class="rd-h3">{i18n.t("rules.invalid.sectionTitle")}</h3>
      {#if isValid}
        <Pill tone="success">{i18n.t("rules.validation.valid")}</Pill>
      {:else}
        <Pill tone="danger">{i18n.t("rules.validation.invalid")}</Pill>
      {/if}
    </section>

    {#if rule !== null || loading}
      <section class="rd-section">
        {#if rule !== null}
          <!-- Neutral markdown as text: no renderer is in scope, and a renderer
               that rewrote characters would invalidate the "what you read is
               what installs" contract (spec §7). Scrollable, so focusable: the
               "noninteractive tabindex" heuristic is silenced on purpose. -->
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <pre class="rd-body" dir="auto" tabindex="0">{rule.body}</pre>
        {:else}
          <p class="rd-status">{i18n.t("common.loading")}</p>
        {/if}
      </section>
    {/if}

    <section class="rd-section rd-preview">
      <h3 class="rd-h3">
        {isValid ? i18n.t("rules.preview.title") : i18n.t("rules.preview.invalidTitle")}
      </h3>

      {#if isValid}
        <div class="rd-preview-head">
          <!-- Container is not a tab stop; the roving `tabindex` lives on the
               radios themselves (standard radiogroup pattern). -->
          <div
            class="rd-scope"
            role="radiogroup"
            tabindex="-1"
            aria-label={i18n.t("rules.scope.label")}
            onkeydown={onScopeKey}
          >
            {#each availableScopes as option (option)}
              <button
                type="button"
                class="rd-scope-opt"
                class:on={activeScope === option}
                role="radio"
                id={`rd-scope-${option}`}
                aria-checked={activeScope === option}
                tabindex={activeScope === option ? 0 : -1}
                onclick={() => setScope(option)}
              >
                {option === "user" ? i18n.t("rules.scope.user") : i18n.t("rules.scope.project")}
              </button>
            {/each}
          </div>

          {#if activeScope === "project"}
            <div class="rd-root">
              <span class="rd-root-label">{i18n.t("rules.scope.projectRoot")}</span>
              {#if projectPath === null}
                <span class="rd-root-none">{i18n.t("rules.scope.projectRootNone")}</span>
                <button type="button" class="rd-link" onclick={chooseProject}>
                  <FolderOpenIcon size={13} />
                  <span>{i18n.t("rules.scope.chooseProject")}</span>
                </button>
              {:else}
                <code class="rd-root-path" dir="ltr">{projectPath}</code>
                <button type="button" class="rd-link" onclick={chooseProject}>
                  {i18n.t("rules.scope.changeProject")}
                </button>
              {/if}
            </div>
          {/if}
        </div>

        <RulePreview
          {entry}
          loading={rules.previewLoading}
          error={rules.previewError}
          harness={activeHarness}
          scope={activeScope}
          {projectPath}
          onHarness={(id) => (chosenHarness = id)}
          onChooseProject={chooseProject}
        />

        <RuleInstallControls
          slug={view.slug}
          name={view.name}
          valid={isValid}
          harness={activeHarness}
          scope={activeScope}
          {projectPath}
          dest={entry?.dest ?? null}
          previewLoading={rules.previewLoading}
          previewError={rules.previewError}
        />
      {:else}
        <div class="rd-invalid">
          <p class="rd-errors-head">{i18n.t("rules.invalid.errorsHeader")}</p>
          <ul class="rd-errors">
            {#each validationErrors as message, index (index)}
              <li>{message}</li>
            {/each}
          </ul>
          <p class="rd-invalid-foot">{i18n.t("rules.preview.invalid")}</p>
        </div>

        <RuleInstallControls
          slug={view.slug}
          name={view.name}
          valid={false}
          harness={activeHarness}
          scope={activeScope}
          {projectPath}
          dest={null}
          previewLoading={false}
          previewError={null}
        />
      {/if}
    </section>

    <!-- Every install the app recorded for this rule — the uninstall surface
         TASK 21 requires: a project install is only findable again here,
         because the picked root is gone once the scope control resets. -->
    {#if installs.length > 0}
      <section class="rd-section">
        <h3 class="rd-h3">{i18n.t("rules.install.installedIn")}</h3>
        <ul class="rd-installs" role="list">
          {#each installs as row (rowKey(row))}
            <li class="rd-install">
              <span class="rd-install-ident">
                <span class="rd-install-harness">{harnessLabel(row.tool)}</span>
                <code class="rd-install-scope" dir="ltr">{installScopeLabel(row)}</code>
              </span>
              <code class="rd-install-dest" dir="ltr">{row.dest}</code>
              <Button
                variant="danger"
                size="sm"
                loading={rules.isBusy(row.slug, row.tool, row.scope, row.projectPath)}
                disabled={rules.busy !== null}
                ariaLabel={i18n.t("rules.install.uninstallAt", {
                  harness: harnessLabel(row.tool),
                  scope: installScopeLabel(row),
                })}
                onclick={() => (uninstallRow = row)}
              >
                {#snippet icon()}<TrashIcon size={14} />{/snippet}
                {i18n.t("rules.uninstall.action")}
              </Button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>

  <!-- Bulk path (#92): the shared destinations × harnesses grid, owned here and
       mounted only while a rule is selected. The inline panel above stays the
       precise path; this never replaces it. -->
  {#if installOpen}
    <InstallModal
      title={i18n.t("rules.install.gridTitle", { rule: view.name })}
      target={{ kind: "rules", ruleSlugs: [view.slug] }}
      onClose={() => (installOpen = false)}
    />
  {/if}
{/if}

<DestructiveConfirm
  open={uninstallRow !== null}
  title={i18n.t("rules.uninstall.confirmTitle", {
    rule: view?.name ?? "",
    harness: uninstallRow === null ? "" : harnessLabel(uninstallRow.tool),
  })}
  confirmLabel={i18n.t("rules.uninstall.action")}
  confirmVariant="danger"
  onConfirm={doUninstallRow}
  onCancel={() => (uninstallRow = null)}
>
  <p>{i18n.t("rules.uninstall.confirm")}</p>
  {#if uninstallRow !== null && ruleHarness(uninstallRow.tool)?.plugin !== undefined}
    <p>
      {i18n.t("rules.uninstall.pluginKept", {
        plugin: ruleHarness(uninstallRow.tool)?.plugin?.package ?? "",
      })}
    </p>
  {/if}
</DestructiveConfirm>

<style>
  .rd { display: flex; flex-direction: column; gap: var(--space-4); }

  .rd-head { display: flex; flex-direction: column; gap: var(--space-2); }
  .rd-ident { display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; }
  /* The grid entry point sits at the trailing edge of the identity row. */
  .rd-grid-open { display: inline-flex; margin-inline-start: auto; }
  .rd-title { font-size: var(--text-h1); font-weight: var(--fw-semibold); color: var(--color-text-primary); }
  .rd-desc { font-size: var(--text-body-sm); color: var(--color-text-secondary); line-height: var(--lh-normal); }

  .rd-section { display: flex; flex-direction: column; gap: var(--space-2); }
  .rd-h3 {
    font-size: var(--text-body);
    font-weight: var(--fw-semibold);
    color: var(--color-text-primary);
  }

  .rd-fm {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-1) var(--space-3);
    margin: 0;
    font-size: var(--text-body-sm);
  }
  .rd-fm dt { color: var(--color-text-muted); }
  .rd-fm dd {
    margin: 0;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    font-size: var(--text-mono);
    word-break: break-word;
  }

  .rd-body {
    margin: 0;
    padding: var(--space-3);
    max-height: 320px;
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    font-size: var(--text-mono);
    line-height: var(--lh-mono);
    white-space: pre-wrap;
  }

  .rd-status { font-size: var(--text-body-sm); color: var(--color-text-muted); }

  .rd-load-error {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: var(--space-3);
    border: 1px solid var(--color-danger);
    border-radius: var(--radius-md);
    background: var(--color-danger-subtle);
  }
  .rd-load-error-title {
    font-size: var(--text-body-sm);
    font-weight: var(--fw-semibold);
    color: var(--color-danger-on-subtle);
  }
  .rd-load-error-body { font-size: var(--text-body-sm); color: var(--color-danger-on-subtle); }

  .rd-preview { gap: var(--space-3); }
  .rd-preview-head { display: flex; flex-direction: column; gap: var(--space-2); }

  .rd-scope {
    display: inline-flex;
    align-self: flex-start;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }
  .rd-scope-opt {
    height: 26px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-secondary);
    font-size: var(--text-body-sm);
    cursor: pointer;
    white-space: nowrap;
  }
  .rd-scope-opt:hover { color: var(--color-text-primary); }
  .rd-scope-opt.on {
    background: var(--color-surface-raised);
    color: var(--color-text-primary);
    font-weight: var(--fw-medium);
    box-shadow: var(--shadow-xs);
  }

  .rd-root { display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; }
  .rd-root-label { font-size: var(--text-caption); color: var(--color-text-muted); }
  .rd-root-none { font-size: var(--text-caption); color: var(--color-text-muted); }
  .rd-root-path {
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--color-text-primary);
    direction: ltr;
    unicode-bidi: isolate;
    word-break: break-all;
  }

  .rd-link {
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
  .rd-link:hover { text-decoration: underline; }

  .rd-invalid { display: flex; flex-direction: column; gap: var(--space-2); }
  .rd-errors-head { font-size: var(--text-caption); color: var(--color-text-muted); }
  .rd-errors {
    list-style: none;
    margin: 0;
    padding: var(--space-2) var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    border: 1px solid var(--color-danger);
    border-radius: var(--radius-md);
    background: var(--color-danger-subtle);
    color: var(--color-danger-on-subtle);
    font-family: var(--font-mono);
    font-size: var(--text-mono);
    line-height: var(--lh-mono);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .rd-invalid-foot { font-size: var(--text-body-sm); color: var(--color-warning-strong); }

  .rd-installs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .rd-install {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
  }
  .rd-install-ident {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }
  .rd-install-harness {
    font-size: var(--text-body-sm);
    font-weight: var(--fw-medium);
    color: var(--color-text-primary);
  }
  /* Paths stay LTR in every locale — a reversed root is the wrong folder. */
  .rd-install-scope {
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--color-text-secondary);
    direction: ltr;
    unicode-bidi: isolate;
    word-break: break-all;
  }
  .rd-install-dest {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--color-text-muted);
    direction: ltr;
    unicode-bidi: isolate;
    word-break: break-all;
  }
</style>
