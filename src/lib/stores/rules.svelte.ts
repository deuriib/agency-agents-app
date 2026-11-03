/**
 * Rules store — the `agency-rules` catalog (`rules.*` commands) behind the Rules
 * workspace: list + status, the selected rule's body, the compiled preview per
 * harness, and the ledger rows that say what is installed where.
 *
 * Every shape here maps to a branch the Rules views actually render (TASK 18's
 * "no dead fields" rule): `loaded` is distinct from `list.length === 0` so
 * "empty" ≠ "not fetched yet", `status.emptyReason` carries the
 * not-synced / no-rules distinction verbatim (spec §8), and `error` keeps a
 * read/parse failure visible instead of degrading to a misleading empty state —
 * the same lesson recorded in `runbooks.svelte.ts` and `decisions.md`
 * 2026-11-02 (#58).
 *
 * Install truth is NEVER held here: `installations` is re-read from
 * `rules_installs` after every mutation and the views derive from it, so a
 * restart shows the same rows the ledger holds (spec §10.6). Rows are matched on
 * `(slug, tool, scope, projectPath)` — `scope` is part of the identity, because
 * a user row and an unpicked project scope both carry `projectPath: null`.
 *
 * Previews are cached in memory only, keyed by `slug + projectPath` (a project
 * root changes every resolved destination), and the cache is dropped whenever
 * the corpus is refreshed or an install/uninstall changes what is on disk.
 *
 * Singleton: import `rules` everywhere.
 */
import { invoke } from "@tauri-apps/api/core";

import type {
  InstallRecord,
  Rule,
  RuleInstall,
  RulePreview,
  RulePreviewEntry,
  RuleStatus,
  RuleSummary,
  Scope,
} from "$lib/types";
import { errorText } from "$lib/types";

/** In-memory preview cache. Never persisted — a stale dest is worse than a refetch. */
const previewCache = new Map<string, RulePreview>();

/** `NUL` cannot appear in a slug or an absolute path, so it is a safe join. */
function previewKey(slug: string, projectPath: string | null): string {
  return `${slug}\u0000${projectPath ?? ""}`;
}

function busyKey(
  slug: string,
  harness: string,
  scope: Scope,
  projectPath: string | null,
): string {
  return `${slug}\u0000${harness}\u0000${scope}\u0000${projectPath ?? ""}`;
}

/**
 * The root a `(scope, projectPath)` pair actually writes through.
 *
 * A project scope without a picked root resolves to `null` — which is also the
 * user-scope ledger key. Every consumer must therefore treat
 * `scope === "project" && root === null` as "no state yet", never as "user
 * scope": collapsing the two is what makes the UI offer an Uninstall that
 * deletes the user-global install (and its ledger row).
 */
function effectiveRoot(scope: Scope, projectPath: string | null): string | null {
  return scope === "project" ? projectPath : null;
}

/** Stale-result guards: only the newest in-flight request may write its result. */
let bodySeq = 0;
let previewSeq = 0;

/** The backend always sets `domain: "rule"` on rule rows (ruling C3); narrow it. */
function isRuleInstall(record: InstallRecord): record is RuleInstall {
  return record.domain === "rule";
}

/** Deterministic (code-unit) ordering — matches the Rust `Ord` for ASCII. */
function byCodeUnit(a: string, b: string): number {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

class RulesStore {
  /** Rule summaries from `rules_list` (bodies omitted). Empty until loaded. */
  list: RuleSummary[] = $state([]);
  /** True once the first load resolves (so "empty" ≠ "not fetched yet"). */
  loaded: boolean = $state(false);
  /** True while a load is in flight. */
  loading: boolean = $state(false);
  /** Read/parse failure, ready to display. Never degrades to the empty state. */
  error: string | null = $state(null);
  /** `rules_status` — distinguishes "not synced" from "no rules shipped". */
  status: RuleStatus | null = $state(null);
  /** Rule rows from `rules_installs` — the ONLY install truth in the UI. */
  installations: RuleInstall[] = $state([]);

  /** Slug of the rule shown in the detail pane. */
  selectedSlug: string | null = $state(null);
  /** Full rule (body + hashes) for `selectedSlug`. */
  detail: Rule | null = $state(null);
  detailLoading: boolean = $state(false);
  detailError: string | null = $state(null);

  /** Compiled output for the last requested `(slug, scope, projectPath)`.
   *  Named `previewResult` because `preview()` is the action that fills it. */
  previewResult: RulePreview | null = $state(null);
  previewLoading: boolean = $state(false);
  previewError: string | null = $state(null);

  /** Search + filters. They compose (AND); `filtersActive()` drives the clear CTA. */
  query: string = $state("");
  harnessFilter: string | null = $state(null);
  categoryFilter: string | null = $state(null);

  /** `${slug}\0${harness}\0${scope}\0${projectPath}` while a mutation is pending. */
  busy: string | null = $state(null);
  /** True while `rules_refresh` runs (drives the empty-state CTAs). */
  refreshing: boolean = $state(false);

  // ── Load ────────────────────────────────────────────────────────────────

  /**
   * Read status + list + ledger in one round trip set. Safe to call on mount;
   * a failure surfaces `error` and never pretends the catalog is empty.
   */
  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    try {
      const [status, list, records] = await Promise.all([
        invoke<RuleStatus>("rules_status"),
        invoke<RuleSummary[]>("rules_list", { category: null }),
        invoke<InstallRecord[]>("rules_installs"),
      ]);
      this.status = status;
      this.list = list;
      this.installations = records.filter(isRuleInstall);
      this.error = null;
    } catch (e) {
      this.list = [];
      this.status = null;
      // Install truth dies with the read that failed: keeping the previous
      // ledger rows would show an installed/uninstallable state the app can no
      // longer confirm (fail closed, spec §10.6).
      this.installations = [];
      this.error = errorText(e);
    } finally {
      this.loaded = true;
      this.loading = false;
    }
  }

  /**
   * Re-fetch the catalog from the `agency-rules` tarball (`rules_refresh`).
   * The only "sync" mechanism for rules, so both empty-state CTAs route here.
   * Throws on failure (offline mode, network) so the caller can toast the
   * verbatim error — the existing list is left untouched on a failed refresh.
   */
  async refreshCorpus(): Promise<void> {
    if (this.refreshing) return;
    this.refreshing = true;
    try {
      await invoke<RuleStatus>("rules_refresh");
      previewCache.clear();
      this.selectedSlug = null;
      this.detail = null;
      this.detailError = null;
      this.previewResult = null;
      this.previewError = null;
      await this.load();
    } finally {
      this.refreshing = false;
    }
  }

  // ── Selection + body ────────────────────────────────────────────────────

  /** Select a rule (or clear the selection) and fetch its body. */
  select(slug: string | null): void {
    if (this.selectedSlug === slug) return;
    this.selectedSlug = slug;
    this.detail = null;
    this.detailError = null;
    this.previewResult = null;
    this.previewError = null;
    if (slug !== null) void this.loadBody(slug);
  }

  /** Fetch one rule's full DTO. A late response for a previous selection is dropped. */
  async loadBody(slug: string): Promise<void> {
    const seq = ++bodySeq;
    this.detailLoading = true;
    this.detailError = null;
    try {
      const rule = await invoke<Rule>("rules_get", { slug });
      if (seq !== bodySeq || this.selectedSlug !== slug) return;
      this.detail = rule;
    } catch (e) {
      if (seq !== bodySeq || this.selectedSlug !== slug) return;
      this.detail = null;
      this.detailError = errorText(e);
    } finally {
      if (seq === bodySeq) this.detailLoading = false;
    }
  }

  // ── Preview ─────────────────────────────────────────────────────────────

  /**
   * Compile `slug` for the given project root (`null` = user scope). Nothing is
   * written; the response is cached per `slug + projectPath` for this session.
   */
  async preview(slug: string, projectPath: string | null): Promise<void> {
    // Bump first: a cache hit must also invalidate any older in-flight request.
    const seq = ++previewSeq;
    const key = previewKey(slug, projectPath);
    const cached = previewCache.get(key);
    if (cached) {
      this.previewResult = cached;
      this.previewError = null;
      this.previewLoading = false;
      return;
    }

    this.previewLoading = true;
    this.previewError = null;
    // Never let the previous rule's bytes sit under this rule's name while the
    // new compile is in flight.
    if (this.previewResult !== null && this.previewResult.slug !== slug) {
      this.previewResult = null;
    }
    try {
      const result = await invoke<RulePreview>("rules_preview", { slug, projectPath });
      previewCache.set(key, result);
      if (seq !== previewSeq) return;
      this.previewResult = result;
    } catch (e) {
      if (seq !== previewSeq) return;
      this.previewResult = null;
      this.previewError = errorText(e);
    } finally {
      if (seq === previewSeq) this.previewLoading = false;
    }
  }

  /**
   * The compiled entry for one harness, only when the cached preview was
   * resolved for the scope and root currently on screen. A user-scope preview
   * must never answer a project-scope question (spec §7).
   */
  previewEntry(
    slug: string,
    harness: string,
    scope: Scope,
    projectPath: string | null,
  ): RulePreviewEntry | null {
    const result = this.previewResult;
    if (result === null || result.slug !== slug) return null;
    if (result.scope !== scope) return null;
    if ((result.projectPath ?? null) !== effectiveRoot(scope, projectPath)) return null;
    return result.harnesses.find((entry) => entry.harness === harness) ?? null;
  }

  // ── Install lifecycle ───────────────────────────────────────────────────

  /**
   * Ledger row for one `(rule, harness, scope)` — install state is per triple.
   *
   * The scope is matched explicitly, not inferred from the path: the backend
   * keys the ledger on `(slug, tool, project_path)` and stores `scope` beside
   * it, so a user row and a project scope with no root share `project_path:
   * null` while meaning different things. Returning `null` for the latter is
   * deliberate — "no project folder picked yet" is not "installed for the
   * user", and pretending it is would let the UI delete the user-global files.
   */
  installFor(
    slug: string,
    harness: string,
    scope: Scope,
    projectPath: string | null,
  ): RuleInstall | null {
    const root = effectiveRoot(scope, projectPath);
    if (scope === "project" && root === null) return null;
    return (
      this.installations.find(
        (row) =>
          row.slug === slug &&
          row.tool === harness &&
          row.scope === scope &&
          (row.projectPath ?? null) === root,
      ) ?? null
    );
  }

  /**
   * Every ledger row for one rule, across harnesses and scopes — the detail
   * pane's "installed in" list. A project install is only findable again if the
   * UI shows it here: the root is otherwise lost with the picker state
   * (TASK 21: "uninstall appears for every (rule, harness, scope) present in
   * `rules_installs`").
   *
   * Ordered code-unit by harness, then user scope before project scope, then
   * by path, so the list does not reshuffle between renders.
   */
  installsFor(slug: string): RuleInstall[] {
    return this.installations
      .filter((row) => row.slug === slug)
      .sort((a, b) => {
        const byTool = byCodeUnit(a.tool, b.tool);
        if (byTool !== 0) return byTool;
        if (a.scope !== b.scope) return a.scope === "user" ? -1 : 1;
        return byCodeUnit(a.projectPath ?? "", b.projectPath ?? "");
      });
  }

  /** Harness ids this rule is installed for, across every scope — the row dots. */
  installedHarnesses(slug: string): Set<string> {
    return new Set(this.installations.filter((row) => row.slug === slug).map((row) => row.tool));
  }

  /** True while THIS `(rule, harness, scope)` mutation is in flight. */
  isBusy(slug: string, harness: string, scope: Scope, projectPath: string | null): boolean {
    return this.busy === busyKey(slug, harness, scope, projectPath);
  }

  /**
   * Install one rule for one harness and one scope, then re-read the ledger.
   *
   * Nothing is set optimistically: the returned record is discarded and the
   * views re-derive from `rules_installs`. The ledger re-read is part of the
   * action on purpose — if the app cannot confirm what it wrote, it must not
   * claim success (fail closed).
   */
  async install(
    slug: string,
    harness: string,
    scope: Scope,
    projectPath: string | null,
  ): Promise<void> {
    const root = effectiveRoot(scope, projectPath);
    // Project scope without a root would reach the backend as `project_path:
    // null`, i.e. a USER-scope write. Refuse loudly instead of writing to `~`.
    if (scope === "project" && root === null) {
      throw new Error("project scope requires a project path");
    }
    // Belt-and-suspenders: the controls disable every button while `busy` is
    // set, so a second submit cannot reach here from the UI.
    if (this.busy !== null) return;
    this.busy = busyKey(slug, harness, scope, root);
    try {
      await invoke<InstallRecord>("rules_install", { slug, harness, projectPath: root });
      previewCache.delete(previewKey(slug, root));
      await this.reloadInstallations();
    } finally {
      this.busy = null;
    }
  }

  /** Remove the files the app wrote for one `(rule, harness, scope)` and its row. */
  async uninstall(
    slug: string,
    harness: string,
    scope: Scope,
    projectPath: string | null,
  ): Promise<void> {
    const root = effectiveRoot(scope, projectPath);
    // Same guard as `install`: a project-scope call without a root would delete
    // the user-scope row and its files.
    if (scope === "project" && root === null) {
      throw new Error("project scope requires a project path");
    }
    if (this.busy !== null) return;
    this.busy = busyKey(slug, harness, scope, root);
    try {
      await invoke<void>("rules_uninstall", { slug, harness, projectPath: root });
      previewCache.delete(previewKey(slug, root));
      await this.reloadInstallations();
    } finally {
      this.busy = null;
    }
  }

  /** Re-read the rule rows from the ledger. Throws so the caller reports it. */
  private async reloadInstallations(): Promise<void> {
    const records = await invoke<InstallRecord[]>("rules_installs");
    this.installations = records.filter(isRuleInstall);
  }

  // ── Filters ─────────────────────────────────────────────────────────────

  setQuery(query: string): void {
    this.query = query;
  }

  setHarnessFilter(harness: string | null): void {
    this.harnessFilter = harness;
  }

  setCategoryFilter(category: string | null): void {
    this.categoryFilter = category;
  }

  clearFilters(): void {
    this.query = "";
    this.harnessFilter = null;
    this.categoryFilter = null;
  }

  /** True when any search/filter narrows the list (drives the "clear" affordance). */
  filtersActive(): boolean {
    return this.query.trim() !== "" || this.harnessFilter !== null || this.categoryFilter !== null;
  }

  /**
   * Search matches `name`, `description`, and `slug` (case-insensitive).
   *
   * The harness filter answers "which rules are deployed to this harness?". The
   * neutral schema carries no per-harness target field — every rule compiles to
   * every installable harness — so a "targets harness X" filter would either be
   * a no-op or need an N+1 `rules_preview` per row (TASK 19 forbids that). The
   * per-rule × per-harness fact the list payload does carry is the ledger, which
   * is what the harness dots in each row already show.
   */
  filtered(): RuleSummary[] {
    const query = this.query.trim().toLowerCase();
    return this.list.filter((rule) => {
      if (
        this.harnessFilter !== null &&
        !this.installations.some(
          (row) => row.slug === rule.slug && row.tool === this.harnessFilter,
        )
      ) {
        return false;
      }
      if (this.categoryFilter !== null && rule.category !== this.categoryFilter) return false;
      if (query === "") return true;
      return (
        rule.name.toLowerCase().includes(query) ||
        rule.description.toLowerCase().includes(query) ||
        rule.slug.toLowerCase().includes(query)
      );
    });
  }

  /** Categories present in the catalog, sorted deterministically. */
  categories(): string[] {
    return [...new Set(this.list.map((rule) => rule.category))].sort(byCodeUnit);
  }

  /** Rules that failed validation — never hidden, only barred from install. */
  invalidCount(): number {
    return this.list.filter((rule) => !rule.validation.valid).length;
  }
}

export const rules = new RulesStore();
