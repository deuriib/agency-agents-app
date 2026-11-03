/**
 * Pure derivations behind the Rules workspace.
 *
 * Every function here is a plain value → value transform: no runes, no
 * `invoke`, no DOM. The store and the components call these instead of
 * re-deriving the same rules inline, which is what makes the install-state
 * semantics testable without a browser or a Tauri runtime (issue #62).
 *
 * The load-bearing invariant lives in {@link effectiveRoot}: a project scope
 * with no picked folder resolves to `null` — the same ledger key the user scope
 * uses — so every consumer must treat it as "no state yet" rather than "user
 * scope". Collapsing the two is how the UI once offered an Uninstall that
 * deleted the user's global rule and its ledger row.
 */
import type {
  InstallRecord,
  RuleInstall,
  RulePreview,
  RulePreviewEntry,
  RuleSummary,
  RuleStatus,
  Scope,
} from "$lib/types";

/** The root a `(scope, projectPath)` pair actually writes through. */
export function effectiveRoot(scope: Scope, projectPath: string | null): string | null {
  return scope === "project" ? projectPath : null;
}

/**
 * True when the scope is project but no folder was picked. There is no install
 * state, no destination, and nothing may be written: `project_path: null` would
 * reach the backend as a user-scope write.
 */
export function needsProjectRoot(scope: Scope, projectPath: string | null): boolean {
  return scope === "project" && projectPath === null;
}

/** The backend always sets `domain: "rule"` on rule rows (ruling C3); narrow it. */
export function isRuleInstall(record: InstallRecord): record is RuleInstall {
  return record.domain === "rule";
}

/**
 * Ledger row for one `(rule, harness, scope)` — install state is per triple.
 *
 * The scope is matched explicitly, never inferred from the path: a user row and
 * an unpicked project scope share `projectPath: null` while meaning different
 * things. The latter returns `null` on purpose.
 */
export function installFor(
  installations: readonly RuleInstall[],
  slug: string,
  harness: string,
  scope: Scope,
  projectPath: string | null,
): RuleInstall | null {
  const root = effectiveRoot(scope, projectPath);
  if (scope === "project" && root === null) return null;
  return (
    installations.find(
      (row) =>
        row.slug === slug &&
        row.tool === harness &&
        row.scope === scope &&
        (row.projectPath ?? null) === root,
    ) ?? null
  );
}

/** Deterministic (code-unit) ordering — matches the Rust `Ord` for ASCII. */
export function byCodeUnit(a: string, b: string): number {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

/**
 * Every ledger row for one rule, across harnesses and scopes — the detail
 * pane's "installed in" list.
 *
 * Ordered code-unit by harness, then user scope before project scope, then by
 * path, so the list does not reshuffle between renders.
 */
export function installsFor(
  installations: readonly RuleInstall[],
  slug: string,
): RuleInstall[] {
  return installations
    .filter((row) => row.slug === slug)
    .sort((a, b) => {
      const byTool = byCodeUnit(a.tool, b.tool);
      if (byTool !== 0) return byTool;
      if (a.scope !== b.scope) return a.scope === "user" ? -1 : 1;
      return byCodeUnit(a.projectPath ?? "", b.projectPath ?? "");
    });
}

/** Harness ids this rule is installed for, across every scope — the row dots. */
export function installedHarnesses(
  installations: readonly RuleInstall[],
  slug: string,
): Set<string> {
  return new Set(installations.filter((row) => row.slug === slug).map((row) => row.tool));
}

export interface RuleFilters {
  query: string;
  harnessFilter: string | null;
  categoryFilter: string | null;
}

/** True when any search/filter narrows the list (drives the "clear" affordance). */
export function filtersActive(filters: RuleFilters): boolean {
  return (
    filters.query.trim() !== "" ||
    filters.harnessFilter !== null ||
    filters.categoryFilter !== null
  );
}

/**
 * Search matches `name`, `description`, and `slug` (case-insensitive); the
 * harness and category filters compose with it (AND).
 *
 * The harness filter answers "which rules are deployed to this harness?" from
 * the ledger — the neutral rule schema carries no per-harness target, so
 * answering it from `rules_preview` would cost one compile per row.
 */
export function filterRules(
  list: readonly RuleSummary[],
  installations: readonly RuleInstall[],
  filters: RuleFilters,
): RuleSummary[] {
  const query = filters.query.trim().toLowerCase();
  return list.filter((rule) => {
    if (
      filters.harnessFilter !== null &&
      !installations.some((row) => row.slug === rule.slug && row.tool === filters.harnessFilter)
    ) {
      return false;
    }
    if (filters.categoryFilter !== null && rule.category !== filters.categoryFilter) return false;
    if (query === "") return true;
    return (
      rule.name.toLowerCase().includes(query) ||
      rule.description.toLowerCase().includes(query) ||
      rule.slug.toLowerCase().includes(query)
    );
  });
}

/** Categories present in the catalog, sorted deterministically. */
export function ruleCategories(list: readonly RuleSummary[]): string[] {
  return [...new Set(list.map((rule) => rule.category))].sort(byCodeUnit);
}

/** Rules that failed validation — never hidden, only barred from install. */
export function invalidCount(list: readonly RuleSummary[]): number {
  return list.filter((rule) => !rule.validation.valid).length;
}

/**
 * The compiled entry for one harness, only when the cached preview was resolved
 * for the scope and root currently on screen. A user-scope preview must never
 * answer a project-scope question.
 */
export function previewEntry(
  result: RulePreview | null,
  slug: string,
  harness: string,
  scope: Scope,
  projectPath: string | null,
): RulePreviewEntry | null {
  if (result === null || result.slug !== slug) return null;
  if (result.scope !== scope) return null;
  if ((result.projectPath ?? null) !== effectiveRoot(scope, projectPath)) return null;
  return result.harnesses.find((entry) => entry.harness === harness) ?? null;
}

/**
 * Why an install cannot run. Keys, not sentences: the component maps them
 * through i18n, so the precedence is testable without a locale.
 *
 * Ordered most-specific-first — an invalid rule and an unpicked project root
 * each get their own reason and never collapse into the generic
 * "this harness cannot be written" line.
 */
export type InstallBlockReason =
  | "invalid"
  | "noProjectRoot"
  | "notInstallable"
  | "previewLoading"
  | "previewFailed";

export interface InstallGate {
  /** `rule.validation.valid`. */
  valid: boolean;
  /** Selected harness, or `null` when the contract declares none. */
  harness: string | null;
  /** The harness registry says this app renders the harness' format. */
  installable: boolean;
  scope: Scope;
  projectPath: string | null;
  previewLoading: boolean;
  previewError: string | null;
  /** Absolute destination from the preview; `null` when there are no bytes. */
  dest: string | null;
}

export function installBlockReason(gate: InstallGate): InstallBlockReason | null {
  if (!gate.valid) return "invalid";
  if (needsProjectRoot(gate.scope, gate.projectPath)) return "noProjectRoot";
  if (gate.harness === null || !gate.installable) return "notInstallable";
  if (gate.previewLoading) return "previewLoading";
  if (gate.previewError !== null) return "previewFailed";
  // Settled with no error and no destination: the app has no path to state.
  if (gate.dest === null) return "notInstallable";
  return null;
}

/**
 * The mutually exclusive list states the workspace renders, in the order the
 * component checks them. Deriving them as data (instead of an `{#if}` ladder)
 * makes "empty" ≠ "not fetched yet" ≠ "load failed" testable.
 */
export type RuleListState =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "notSynced" }
  | { kind: "noRules" }
  | { kind: "filteredToZero" }
  | { kind: "rows" };

export interface RuleListInput {
  loaded: boolean;
  loading: boolean;
  error: string | null;
  listLength: number;
  status: RuleStatus | null;
  visibleLength: number;
}

export function ruleListState(input: RuleListInput): RuleListState {
  if (!input.loaded || input.loading) return { kind: "loading" };
  if (input.error !== null) return { kind: "error", message: input.error };
  if (input.listLength === 0) {
    return input.status?.emptyReason === "notSynced" ? { kind: "notSynced" } : { kind: "noRules" };
  }
  if (input.visibleLength === 0) return { kind: "filteredToZero" };
  return { kind: "rows" };
}
