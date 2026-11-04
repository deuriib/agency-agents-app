<script lang="ts">
  /**
   * InstallModal — the ONE place agents and rules get deployed. A destinations ×
   * columns grid: rows are destinations (Global + each registered project),
   * columns are the tools (agents) or the rule harnesses (rules), and every cell
   * is a toggle that installs/removes the payload into that (scope, column).
   *
   * Driven by an `InstallTarget` discriminated union: `{ kind: "agents" }` for a
   * single agent, a division, or a team; `{ kind: "rules" }` for one rule's
   * destinations × harnesses grid. The agent branch is the original surface,
   * byte for byte — only the rule branch adds a state machine.
   *
   * Install truth is NEVER held here: every on/off answer is derived from the
   * ledger (`rules.installations` / agent `install.installed`). The only local
   * state is the per-row preview cache (destinations), a transient action, and
   * its per-cell result.
   *
   * "Global" only offers user-capable columns (Cursor's global cell is blank —
   * its global rules are UI-only). Removal of `foreign` agent files asks first.
   */
  import { onMount, untrack } from "svelte";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import GlobeIcon from "@lucide/svelte/icons/globe";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import DestructiveConfirm from "./DestructiveConfirm.svelte";
  import {
    harnessLabel,
    installableHarnesses,
    isHarnessInstallable,
    ruleHarness,
  } from "$lib/data/ruleRegistry";
  import {
    installBlockReason,
    previewEntry,
    ruleCellCoverage,
    ruleCellTargets,
    type InstallBlockReason,
    type RuleCellCoverage,
  } from "$lib/rules/derive";
  import { corpus } from "$lib/stores/corpus.svelte";
  import { install, SUPPORTED_TOOLS, type ToolDef } from "$lib/stores/install.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { rules } from "$lib/stores/rules.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { i18n } from "$lib/stores/i18n.svelte";
  import {
    errorText,
    type Agent,
    type InstallTarget,
    type InstalledAgent,
    type RulePreview,
    type RulePreviewEntry,
    type RuleTarget,
    type Scope,
    type Tool,
  } from "$lib/types";

  interface Props {
    title: string;
    target: InstallTarget;
    onClose: () => void;
  }
  let { title, target, onClose }: Props = $props();

  const isRule = $derived(target.kind === "rules");
  const ruleSlugs = $derived(target.kind === "rules" ? target.ruleSlugs : []);
  const agentSlugs = $derived(target.kind === "agents" ? target.agentSlugs : []);

  onMount(() => {
    projects.refresh();
    // Tool detection is agent-only work; a rules surface must not run
    // `tools_list` / `installs_reconcile` for nothing (D9).
    if (!isRule) void install.loadTools();
  });

  // The agents in this set that exist in the corpus (stale slugs skipped).
  const slugSet = $derived(new Set(agentSlugs));
  const agents = $derived<Agent[]>(corpus.agents.filter((a) => slugSet.has(a.slug)));
  const total = $derived(agents.length);

  /** Deduplicated: a duplicated slug would make `all` unreachable. */
  const ruleSlugSet = $derived(new Set(ruleSlugs));
  /** Unknown slugs do not block — the preview will reject them with a reason. */
  const ruleValid = $derived(
    ruleSlugs.every((slug) => rules.list.find((r) => r.slug === slug)?.validation.valid ?? true),
  );

  // Columns = tools present on this device (detected, or already holding an
  // install of this set), that can take an agent in SOME scope.
  function detected(t: ToolDef): boolean {
    return (
      install.tools.length === 0 ||
      install.tools.some((ti) => ti.tool === t.id && ti.detected) ||
      install.installed.some((r) => r.tool === t.id && r.state !== "removed" && slugSet.has(r.slug))
    );
  }

  /**
   * One normalized column type for both modes (D3). `RuleHarnessMeta` has no
   * `supportsUser/supportsProject`, so `applicable`/`naReason` cannot read the
   * shape directly; mapping each source keeps agent copy and behaviour
   * identical while rule mode never filters by `detected()`.
   */
  type GridCol = { id: string; label: string; user: boolean; project: boolean };

  const cols = $derived<GridCol[]>(
    isRule
      ? installableHarnesses().map((h) => ({
          id: h.id,
          label: h.label,
          user: h.dest.user !== undefined,
          project: h.dest.project !== undefined,
        }))
      : SUPPORTED_TOOLS.filter((t) => (t.supportsUser || t.supportsProject) && detected(t)).map(
          (t) => ({ id: t.id, label: t.label, user: t.supportsUser, project: t.supportsProject }),
        ),
  );

  // Rows = Global + each registered/used project.
  type Row = { kind: "global" } | { kind: "project"; path: string; label: string };

  /**
   * Rule-ledger project roots. `projects.list` unions only the AGENT ledger, so
   * a rule installed into a folder later unregistered from Projects would lose
   * its grid row and could only be removed from the detail pane (D4).
   */
  const rulePaths = $derived([
    ...new Set(
      rules.installations
        .map((r) => r.projectPath)
        .filter((p): p is string => p !== null && p !== ""),
    ),
  ]);

  const projectRows = $derived.by((): Row[] => {
    if (!isRule) {
      return projects.list.map((p) => ({ kind: "project" as const, path: p.path, label: p.label }));
    }
    const labelByPath = new Map(projects.list.map((p) => [p.path, p.label]));
    const extra = rulePaths
      .filter((path) => !labelByPath.has(path))
      .sort((a, b) => a.localeCompare(b));
    return [
      ...projects.list.map((p) => ({ kind: "project" as const, path: p.path, label: p.label })),
      ...extra.map((path) => ({ kind: "project" as const, path, label: labelOf(path) })),
    ];
  });

  const rows = $derived<Row[]>([{ kind: "global" }, ...projectRows]);

  function targetOf(row: Row): string | null {
    return row.kind === "global" ? null : row.path;
  }

  /** Stable key for the per-row preview cache and the `{#each}` identity. */
  function rowId(row: Row): string {
    return row.kind === "global" ? "global" : `project:${row.path}`;
  }

  function applicable(row: Row, col: GridCol): boolean {
    return row.kind === "global" ? col.user : col.project;
  }

  // Columns in this grid that ONLY install per-project (no global scope) — e.g.
  // Cursor, whose "global" rules are a UI setting, not a writable file. With no
  // projects registered their only cell is the Global-row "—", a dead end (#40),
  // so we name them and steer the user to add a project.
  const projectOnlyCols = $derived(cols.filter((col) => col.project && !col.user));
  const noProjects = $derived(
    isRule ? rulePaths.length === 0 && projects.list.length === 0 : projects.list.length === 0,
  );

  /** Why a cell shows "—" (not installable there), for its tooltip + a11y. */
  function naReason(row: Row, col: GridCol): string {
    return row.kind === "global"
      ? i18n.t("install.naProjectOnly", { tool: col.label })
      : i18n.t("install.naUserOnly", { tool: col.label });
  }

  // Coverage of the set in one (tool, target) cell.
  function cover(tool: Tool, target: string | null) {
    const rs = install.installed.filter(
      (r) => r.state !== "removed" && slugSet.has(r.slug) && r.tool === tool && (r.projectPath ?? null) === target,
    );
    const present = new Set(rs.map((r) => r.slug));
    return {
      rows: rs,
      count: present.size,
      all: total > 0 && present.size === total,
      some: present.size > 0 && present.size < total,
      hasForeign: rs.some((r) => r.state === "foreign"),
    };
  }

  let busy = $state<string | null>(null);
  const cellKey = (tool: Tool, target: string | null) => `${tool}:${target ?? ""}`;
  let confirm = $state<{ tool: Tool; target: string | null; rows: InstalledAgent[] } | null>(null);

  async function toggle(tool: Tool, target: string | null) {
    if (busy) return;
    const cov = cover(tool, target);
    if (cov.all) {
      if (cov.hasForeign) {
        confirm = { tool, target, rows: cov.rows };
        return;
      }
      await remove(tool, target, cov.rows);
      return;
    }
    const present = new Set(cov.rows.map((r) => r.slug));
    const missing = agents.filter((a) => !present.has(a.slug));
    if (missing.length === 0) return;
    busy = cellKey(tool, target);
    try {
      const { ok, fail } = await install.bulk(
        "install",
        missing.map((a) => ({ slug: a.slug, tool, projectPath: target })),
      );
      const where = target ? labelOf(target) : i18n.t("common.global");
      if (fail === 0) toast.success(i18n.t("install.installedToast", { count: ok, tool: install.toolLabel(tool), where }));
      else toast.error(i18n.t("install.installFailedToast", { tool: install.toolLabel(tool), ok, fail }));
    } finally {
      busy = null;
    }
  }

  async function remove(tool: Tool, target: string | null, rs: InstalledAgent[]) {
    busy = cellKey(tool, target);
    try {
      const { ok, fail } = await install.bulk(
        "uninstall",
        rs.map((r) => ({ slug: r.slug, tool: r.tool, projectPath: r.projectPath })),
      );
      if (fail === 0) toast.success(i18n.t("install.removedToast", { count: ok, tool: install.toolLabel(tool) }));
      else toast.error(i18n.t("install.removeFailedToast", { tool: install.toolLabel(tool), ok, fail }));
    } finally {
      busy = null;
    }
  }

  async function confirmRemove() {
    if (!confirm) return;
    const { tool, target, rows: rs } = confirm;
    confirm = null;
    await remove(tool, target, rs);
  }

  function labelOf(path: string): string {
    return path.replace(/\/+$/, "").split("/").pop() || path;
  }

  // ── Rule mode ───────────────────────────────────────────────────────────

  /** Destinations for one (row, slug) pair: a preview either resolves or rejects. */
  type RowPreview = { loading: boolean; error: string | null; result: RulePreview | null };

  /**
   * Per-row, per-slug destination cache: `rowId → slug → preview`.
   *
   * One slot per row would be wrong for a multi-rule payload — each slug compiles
   * to its OWN destination set, so the last slug to resolve would overwrite every
   * earlier one and their cells would be labelled with a foreign destination (or
   * none). Each slug therefore owns its own entry from the first paint.
   *
   * `$state.raw` because the value is replaced as a whole; `previewsRef` is the
   * non-reactive mirror the effect writes through, so a completion can merge
   * without READING the reactive map inside the same effect (which would
   * retrigger it forever — D8).
   */
  type RowPreviewCache = Record<string, Record<string, RowPreview>>;
  let previews = $state.raw<RowPreviewCache>({});
  let previewsRef: RowPreviewCache = {};
  let previewRun = 0;

  type Cell = {
    coverage: RuleCellCoverage;
    busy: boolean;
    disabled: boolean;
    dest: string | null;
    reasonKey: InstallBlockReason | null;
    reason: string | null;
  };

  type Action = {
    kind: "install" | "remove";
    harness: string;
    target: string | null;
    targets: RuleTarget[];
  };

  type CellResult = {
    kind: "install" | "remove";
    harness: string;
    target: string | null;
    ok: number;
    fail: number;
    errors: string[];
  };

  /** One action at a time; the confirm dialog and its payload. */
  let action = $state<Action | null>(null);
  let acting = $state(false);
  /** Outcome text for the last action, keyed by cell. Cleared on each action. */
  let lastResult = $state<Record<string, CellResult>>({});

  /**
   * One preview cache per row (one entry per slug inside it), keyed on the row
   * IDS + slug set + validity. Keying on the ID string (not the array identity)
   * matters: `projects.refresh()` swaps in an equal list on mount, and an
   * array-keyed effect would refetch every row for nothing.
   */
  const rowPlanKey = $derived(rows.map((row) => rowId(row)).join("\u0000"));

  $effect(() => {
    if (!isRule) return;
    const run = ++previewRun;
    void rowPlanKey;
    const plan = untrack(() => rows.map((row) => ({ id: rowId(row), root: targetOf(row) })));
    const slugs = [...ruleSlugSet];
    const valid = ruleValid;

    // Every (row, slug) pair starts in its own slot — loading while the preview
    // resolves, terminally idle when the payload is already invalid. Filling all
    // pairs up front means a completion can only ever land in its own entry.
    previewsRef = Object.fromEntries(
      plan.map((entry) => [
        entry.id,
        Object.fromEntries(
          slugs.map((slug) => [slug, { loading: valid, error: null, result: null }]),
        ),
      ]),
    );
    previews = previewsRef;
    if (!valid || slugs.length === 0) return;

    void (async () => {
      for (const entry of plan) {
        for (const slug of slugs) {
          try {
            const result = await rules.previewFor(slug, entry.root);
            if (run !== previewRun) return;
            writePreview(entry.id, slug, { loading: false, error: null, result });
          } catch (e) {
            if (run !== previewRun) return;
            writePreview(entry.id, slug, { loading: false, error: errorText(e), result: null });
          }
        }
      }
    })();
  });

  /** Merge one (row, slug) result without reading the reactive map. */
  function writePreview(rowKey: string, slug: string, next: RowPreview): void {
    const row = previewsRef[rowKey] ?? {};
    previewsRef = { ...previewsRef, [rowKey]: { ...row, [slug]: next } };
    previews = previewsRef;
  }

  /**
   * The row's aggregate preview state: loading while ANY slug in the payload is
   * still resolving, and the first slug error once one rejected. A row therefore
   * never reads as settled while a part of its payload is unresolved, and with a
   * single slug this is the old one-slot behaviour unchanged.
   */
  function rowPreview(row: Row): { loading: boolean; error: string | null } {
    const slots = previews[rowId(row)];
    // Before the effect's first pass there is no cache entry; a valid rule is
    // still resolving, so report loading rather than a false "no destination".
    if (slots === undefined) return { loading: ruleValid, error: null };
    return {
      loading: ruleSlugs.some((slug) => slots[slug]?.loading ?? false),
      error: ruleSlugs.reduce<string | null>(
        (first, slug) => first ?? slots[slug]?.error ?? null,
        null,
      ),
    };
  }

  /**
   * The row's compiled entry for one harness. Every slug is consulted against its
   * OWN cached preview, in payload order; the first match wins. `derive.previewEntry`
   * is the guard (D12): a user-scope preview must never answer a project row, and
   * one slug's preview must never answer another slug's cell.
   */
  function previewEntryFor(row: Row, col: GridCol): RulePreviewEntry | null {
    const slots = previews[rowId(row)];
    if (slots === undefined) return null;
    const scope: Scope = row.kind === "global" ? "user" : "project";
    const root = targetOf(row);
    for (const slug of ruleSlugs) {
      const result = slots[slug]?.result ?? null;
      if (result === null) continue;
      const entry = previewEntry(result, slug, col.id, scope, root);
      if (entry !== null) return entry;
    }
    return null;
  }

  /** The five ready-to-display reason lines; keys come from `installBlockReason`. */
  function reasonText(key: InstallBlockReason | null): string | null {
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
  }

  /**
   * Install state, destination, and block reason for one cell.
   *
   * Enablement (D1): a reason gates INSTALL only. An already-installed cell
   * stays clickable when its row's preview failed, so the ledger row can still
   * be removed — the inline panel behaves the same way.
   */
  function cellFor(row: Row, col: GridCol): Cell {
    const scope: Scope = row.kind === "global" ? "user" : "project";
    const root = targetOf(row);
    const slugs = [...ruleSlugSet];
    const coverage = ruleCellCoverage(rules.installations, slugs, col.id, scope, root);
    const cellBusy = acting || ruleSlugs.some((slug) => rules.isBusy(slug, col.id, scope, root));
    const preview = rowPreview(row);
    const entry = previewEntryFor(row, col);
    // An installed cell keeps the ledger's own destination: the preview may be
    // gone (failed) while the files are still on disk.
    const dest = coverage.all
      ? (coverage.rows[0]?.dest ?? entry?.dest ?? null)
      : (entry?.dest ?? null);
    const harness = ruleHarness(col.id);
    const reasonKey = installBlockReason({
      valid: ruleValid,
      harness: col.id,
      installable: harness !== null && isHarnessInstallable(harness),
      scope,
      projectPath: root,
      previewLoading: preview.loading,
      previewError: preview.error,
      dest: entry?.dest ?? null,
    });
    const reason = reasonText(reasonKey);
    return {
      coverage,
      busy: cellBusy,
      disabled: cellBusy || (!coverage.all && reason !== null),
      dest,
      reasonKey,
      reason,
    };
  }

  /**
   * Composed accessible name: harness + destination + state, plus the block
   * reason when the cell is disabled (disabled-without-a-reason is a defect).
   */
  function cellLabel(row: Row, col: GridCol, cell: Cell): string {
    const key = cell.coverage.all
      ? "rules.install.cellOn"
      : cell.coverage.some
        ? "rules.install.cellPartial"
        : "rules.install.cellOff";
    const target = row.kind === "global" ? i18n.t("common.global") : row.label;
    const base = i18n.t(key, { harness: col.label, target, dest: cell.dest ?? "—" });
    return cell.disabled && cell.reason !== null ? `${base} — ${cell.reason}` : base;
  }

  function openAction(row: Row, col: GridCol): void {
    if (acting) return;
    const scope: Scope = row.kind === "global" ? "user" : "project";
    const root = targetOf(row);
    const coverage = ruleCellCoverage(rules.installations, [...ruleSlugSet], col.id, scope, root);
    const targets = coverage.all
      ? coverage.rows.map((installed) => ({
          slug: installed.slug,
          harness: installed.tool,
          scope: installed.scope,
          projectPath: installed.projectPath,
        }))
      : ruleCellTargets(rules.installations, [...ruleSlugSet], col.id, scope, root);
    if (targets.length === 0) return;
    action = {
      kind: coverage.all ? "remove" : "install",
      harness: col.id,
      target: root,
      targets,
    };
  }

  /**
   * One action, one toast. Success is emitted only when every target landed and
   * at least one did; a refusal or a ledger re-read failure is `gridFailed`,
   * never success (D2/§4).
   */
  async function runAction(): Promise<void> {
    const pending = action;
    action = null;
    if (pending === null) return;
    const key = cellKey(pending.harness, pending.target);
    lastResult = {};
    acting = true;
    try {
      const result = await rules.apply(
        pending.kind === "install" ? "install" : "uninstall",
        pending.targets,
      );
      lastResult = {
        [key]: {
          kind: pending.kind,
          harness: pending.harness,
          target: pending.target,
          ok: result.ok,
          fail: result.fail,
          errors: result.failures.map((failure) => failure.error),
        },
      };
      if (result.fail === 0 && result.ok > 0) {
        toast.success(
          i18n.t(
            pending.kind === "install"
              ? "rules.install.gridInstalled"
              : "rules.install.gridRemoved",
            { count: result.ok },
          ),
        );
      } else if (result.fail > 0) {
        toast.error(i18n.t("rules.install.gridPartial", { ok: result.ok, fail: result.fail }));
      }
    } catch (e) {
      lastResult = {
        [key]: {
          kind: pending.kind,
          harness: pending.harness,
          target: pending.target,
          ok: 0,
          fail: pending.targets.length,
          errors: [errorText(e)],
        },
      };
      toast.error(i18n.t("rules.install.gridFailed", { error: errorText(e) }));
    } finally {
      acting = false;
    }
  }

  function resultCounts(result: CellResult): string {
    if (result.fail === 0) {
      return i18n.t(
        result.kind === "install" ? "rules.install.gridInstalled" : "rules.install.gridRemoved",
        { count: result.ok },
      );
    }
    return i18n.t("rules.install.gridPartial", { ok: result.ok, fail: result.fail });
  }

  const resultLines = $derived(
    Object.entries(lastResult).map(([key, value]) => ({ key, ...value })),
  );

  const actionTitle = $derived(
    action === null
      ? ""
      : i18n.t(
          action.kind === "install"
            ? "rules.install.gridInstallTitle"
            : "rules.install.gridRemoveTitle",
          { count: action.targets.length },
        ),
  );

  const actionPlugin = $derived(
    action === null ? null : (ruleHarness(action.harness)?.plugin ?? null),
  );

  // ── "Add project…" popover ──
  // Instead of firing a native folder dialog on click (which feels broken in the
  // dev shim and is easy to spam), open a list of the projects you already manage
  // — clicking one jumps to its grid row — with an explicit "New Project…" that
  // opens the picker only when you mean it.
  let addOpen = $state(false);
  let flashPath = $state<string | null>(null);
  const destEls: Record<string, HTMLElement> = {};

  function regDest(node: HTMLElement, path: string | null) {
    if (path) destEls[path] = node;
    return { destroy() { if (path) delete destEls[path]; } };
  }

  function flash(path: string) {
    flashPath = path;
    setTimeout(() => { if (flashPath === path) flashPath = null; }, 1200);
  }

  function jumpTo(path: string) {
    addOpen = false;
    destEls[path]?.scrollIntoView({ block: "nearest" });
    flash(path);
  }

  async function newProject() {
    addOpen = false;
    const p = await projects.addViaPicker();
    if (p) {
      await projects.refresh();
      flash(p);
    }
  }
</script>

<Modal open {title} size="wide" onClose={onClose}>
  <p class="sub">
    {isRule
      ? i18n.t("rules.install.gridSub", { count: ruleSlugs.length })
      : i18n.t("install.sub", { count: total })}
  </p>

  {#if cols.length === 0}
    <p class="no-tools">{i18n.t("install.noTools")}</p>
  {:else}
  <div class="grid-wrap">
  <div
    class="grid"
    style="--cols: {cols.length}"
    aria-busy={isRule && acting ? "true" : undefined}
  >
    <!-- header row -->
    <div class="cell head corner"></div>
    {#each cols as col (col.id)}
      <div class="cell head tool" title={col.label}>{col.label}</div>
    {/each}

    {#each rows as row (row.kind === "global" ? "global" : row.path)}
      <div class="cell dest" class:flash={flashPath !== null && targetOf(row) === flashPath} use:regDest={targetOf(row)}>
        {#if row.kind === "global"}
          <span class="d-ic"><GlobeIcon size={15} /></span>
          <span class="d-body"><span class="d-label">{i18n.t("common.global")}</span><span class="d-path">{i18n.t("common.everyMachine")}</span></span>
        {:else}
          <span class="d-ic"><FolderIcon size={15} /></span>
          <span class="d-body"><span class="d-label">{row.label}</span><span class="d-path" title={row.path}>{row.path}</span></span>
        {/if}
      </div>
      {#each cols as col (col.id)}
        {#if applicable(row, col)}
          {#if isRule}
            {@const cell = cellFor(row, col)}
            {@const label = cellLabel(row, col, cell)}
            <button
              class="cell toggle"
              class:on={cell.coverage.all}
              class:partial={cell.coverage.some}
              disabled={cell.disabled}
              title={cell.disabled && cell.reason !== null ? cell.reason : label}
              aria-label={label}
              aria-pressed={cell.coverage.all}
              onclick={() => openAction(row, col)}
            >
              {#if cell.busy}<span class="dot busy"></span>
              {:else if cell.coverage.all}<span class="dot full"></span>
              {:else if cell.coverage.some}<span class="dot half"></span>
              {:else}<span class="dot"></span>{/if}
            </button>
          {:else}
            {@const cov = cover(col.id, targetOf(row))}
            {@const isBusy = busy === cellKey(col.id, targetOf(row))}
            <button
              class="cell toggle"
              class:on={cov.all}
              class:partial={cov.some}
              disabled={isBusy || total === 0}
              title={i18n.t("install.cellTitle", { tool: col.label, target: row.kind === "global" ? i18n.t("common.global") : row.label })}
              aria-label={i18n.t(cov.all ? "install.removeFromAria" : "install.installIntoAria", { tool: col.label, target: row.kind === "global" ? i18n.t("install.globally") : i18n.t("install.inProject", { project: row.label }) })}
              onclick={() => toggle(col.id, targetOf(row))}
            >
              {#if isBusy}<span class="dot busy"></span>
              {:else if cov.all}<span class="dot full"></span>
              {:else if cov.some}<span class="dot half"></span>
              {:else}<span class="dot"></span>{/if}
            </button>
          {/if}
        {:else}
          <div class="cell na" title={naReason(row, col)} aria-label={naReason(row, col)}>—</div>
        {/if}
      {/each}
    {/each}
  </div>
  </div>
  {/if}

  {#if isRule}
    <!--
      Created empty and kept mounted: a live region that is inserted together
      with its text is not reliably announced. The content is the only thing that
      appears/disappears (agent mode keeps its original markup — no live region).
    -->
    <p
      class="grid-status"
      class:blank={resultLines.length === 0}
      role="status"
      aria-live="polite"
    >
      {#if resultLines.length > 0}
        {#each resultLines as result (result.key)}
          <span
            title={result.errors.length > 0
              ? result.errors.join("\n")
              : `${harnessLabel(result.harness)} · ${result.target === null ? i18n.t("common.global") : labelOf(result.target)}`}
          >{resultCounts(result)}</span>
        {/each}
      {/if}
    </p>
  {/if}

  {#if isRule && noProjects}
    <p class="scope-hint">
      <FolderPlus size={13} />
      <span>{i18n.t("rules.install.gridProjectHint")}</span>
    </p>
  {:else if !isRule && cols.length > 0 && projectOnlyCols.length > 0 && noProjects}
    <p class="scope-hint">
      <FolderPlus size={13} />
      <span>{i18n.t("install.projectOnlyHint", { tools: projectOnlyCols.map((col) => col.label).join(", ") })}</span>
    </p>
  {/if}

  <div class="add-wrap">
    <button class="addrow" onclick={() => (addOpen = !addOpen)} aria-haspopup="menu" aria-expanded={addOpen}>
      <FolderPlus size={14} /> {i18n.t("install.addProject")}
    </button>
    {#if addOpen}
      <button class="add-scrim" aria-label={i18n.t("common.close")} onclick={() => (addOpen = false)}></button>
      <div class="add-menu" role="menu">
        {#if projects.list.length > 0}
          <p class="add-head">{i18n.t("install.yourProjects")}</p>
          {#each projects.list as p (p.path)}
            <button class="add-opt" role="menuitem" onclick={() => jumpTo(p.path)}>
              <FolderIcon size={14} />
              <span class="add-body">
                <span class="add-label">{p.label}</span>
                <span class="add-path" title={p.path}>{p.path}</span>
              </span>
            </button>
          {/each}
          <div class="add-div"></div>
        {/if}
        <button class="add-opt new" role="menuitem" onclick={newProject}>
          <FolderPlus size={14} /> <span>{i18n.t("install.newProject")}</span>
        </button>
      </div>
    {/if}
  </div>

  {#snippet actions()}
    <span class="legend"><span class="dot full"></span> {i18n.t("common.installed")} <span class="dot half"></span> {i18n.t("common.some")} <span class="dot"></span> {i18n.t("common.none")}</span>
    <Button variant="primary" onclick={onClose}>{i18n.t("common.done")}</Button>
  {/snippet}
</Modal>

{#if confirm}
  {@const n = confirm.rows.length}
  {@const label = install.toolLabel(confirm.tool)}
  <DestructiveConfirm
    open
    title={i18n.t("install.deleteTitle", { count: n, label })}
    confirmLabel={i18n.t("install.deleteConfirm", { count: n })}
    cancelLabel={i18n.t("common.cancel")}
    onConfirm={confirmRemove}
    onCancel={() => (confirm = null)}
  >
    <p>
      {i18n.t("install.deleteBody", { count: n })}
    </p>
  </DestructiveConfirm>
{/if}

{#if action !== null}
  <DestructiveConfirm
    open
    title={actionTitle}
    confirmLabel={action.kind === "install" ? i18n.t("rules.install.action") : i18n.t("rules.uninstall.action")}
    cancelLabel={i18n.t("common.cancel")}
    confirmVariant={action.kind === "install" ? "primary" : "danger"}
    onConfirm={() => void runAction()}
    onCancel={() => (action = null)}
  >
    {#if action.kind === "remove"}
      <p>{i18n.t("rules.uninstall.confirm")}</p>
      {#if actionPlugin !== null}
        <p>{i18n.t("rules.uninstall.pluginKept", { plugin: actionPlugin.package })}</p>
      {/if}
    {/if}
  </DestructiveConfirm>
{/if}

<style>
  .sub { font-size: var(--text-body-sm); color: var(--color-text-muted); margin-bottom: var(--space-3); }
  .no-tools { font-size: var(--text-body-sm); color: var(--color-text-muted); }

  /* Horizontal scroll guards against a very wide tool set (≫ the modal width). */
  .grid-wrap { overflow-x: auto; border: 1px solid var(--color-border); border-radius: var(--radius-md); }
  .grid {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) repeat(var(--cols), 68px);
    width: max-content; min-width: 100%;
    align-items: stretch;
  }
  .cell { display: flex; align-items: center; justify-content: center; padding: var(--space-2); border-bottom: 1px solid var(--color-border); }
  .head { background: var(--color-surface-sunken); font-size: var(--text-caption); color: var(--color-text-muted); font-weight: var(--fw-semibold); min-height: 34px; }
  .head.tool { writing-mode: horizontal-tb; text-align: center; padding: var(--space-2) 8px; line-height: 1.15; }
  .corner { background: var(--color-surface-sunken); }

  .dest { justify-content: flex-start; gap: var(--space-2); min-width: 0; }
  .d-ic { flex: none; display: inline-flex; color: var(--color-text-secondary); }
  .d-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 0; }
  .d-label { font-size: var(--text-body-sm); font-weight: var(--fw-medium); color: var(--color-text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .d-path { font-size: var(--text-caption); color: var(--color-text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .toggle { background: transparent; cursor: pointer; }
  .toggle:hover:not(:disabled) { background: var(--color-surface-sunken); }
  .toggle:disabled { cursor: default; }
  .na { color: var(--color-text-muted); opacity: 0.4; }

  .dot { width: 16px; height: 16px; border-radius: 999px; border: 1.5px solid var(--color-border-strong, var(--color-text-muted)); box-sizing: border-box; }
  .dot.full { background: var(--color-brand); border-color: var(--color-brand); }
  .dot.half { border-color: var(--color-brand); background: linear-gradient(90deg, var(--color-brand) 50%, transparent 50%); }
  .dot.busy { border-color: var(--color-text-muted); border-top-color: transparent; animation: spin 0.6s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .grid-status {
    display: flex; flex-wrap: wrap; gap: var(--space-3);
    margin-top: var(--space-2);
    font-size: var(--text-caption); color: var(--color-text-secondary);
  }
  /* The region is always mounted in rule mode (live-region reliability); an
     empty one must not leave a gap behind. */
  .grid-status.blank { margin-top: 0; }

  .scope-hint {
    display: flex; align-items: center; gap: 7px;
    margin-top: var(--space-3); padding: var(--space-2) var(--space-3);
    background: color-mix(in srgb, var(--color-brand) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-brand) 22%, transparent);
    border-radius: var(--radius-md);
    font-size: var(--text-body-sm); color: var(--color-text-secondary);
  }
  .scope-hint :global(svg) { flex: none; color: var(--color-brand); }

  .add-wrap { position: relative; display: inline-block; margin-top: var(--space-2); }
  .addrow {
    display: inline-flex; align-items: center; gap: 6px;
    padding: var(--space-2);
    background: transparent; color: var(--color-brand); font-size: var(--text-body-sm); cursor: pointer;
  }
  .addrow:hover { text-decoration: underline; }

  /* Backdrop closes the popover on any outside click. */
  .add-scrim { position: fixed; inset: 0; z-index: 1; background: transparent; border: 0; cursor: default; }
  .add-menu {
    position: absolute; bottom: calc(100% + 4px); left: 0; z-index: 2;
    min-width: 260px; max-width: 360px; max-height: 280px; overflow-y: auto; padding: 4px;
    background: var(--color-surface-raised); border: 1px solid var(--color-border);
    border-radius: var(--radius-md); box-shadow: var(--shadow-lg);
    display: flex; flex-direction: column; gap: 1px;
  }
  .add-head {
    padding: 6px 8px 2px; font-size: var(--text-caption); font-weight: var(--fw-semibold);
    color: var(--color-text-muted); text-transform: uppercase; letter-spacing: 0.04em;
  }
  .add-opt {
    display: flex; align-items: center; gap: var(--space-2);
    padding: 6px 8px; border-radius: var(--radius-sm);
    background: transparent; color: var(--color-text-primary);
    font-size: var(--text-body-sm); text-align: left; cursor: pointer; min-width: 0;
  }
  .add-opt:hover { background: var(--color-surface-sunken); }
  .add-opt.new { color: var(--color-brand); font-weight: var(--fw-medium); }
  .add-body { display: flex; flex-direction: column; gap: 0; min-width: 0; }
  .add-label { font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .add-path { font-size: var(--text-caption); color: var(--color-text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .add-div { height: 1px; margin: 4px 0; background: var(--color-border); }

  .dest.flash { animation: destFlash 1.2s var(--motion-ease-out, ease-out); }
  @keyframes destFlash {
    0%, 100% { background: transparent; }
    25% { background: var(--color-brand-subtle, color-mix(in srgb, var(--color-brand) 16%, transparent)); }
  }

  .legend { display: inline-flex; align-items: center; gap: 6px; margin-right: auto; font-size: var(--text-caption); color: var(--color-text-muted); }
  .legend .dot { width: 13px; height: 13px; }
</style>
