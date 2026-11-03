/**
 * Rule harness registry — the SINGLE source of truth for the rule harnesses.
 *
 * Loaded from `src-tauri/data/rules-tools.json` (a verbatim copy of the upstream
 * `agency-rules/tools.json` `harnesses` object) and
 * `src-tauri/data/rule-formats.json`, the same two files the Rust backend
 * embeds with `include_str!`. Whether THIS app can install a harness is
 * **derived**, never stored: a harness is installable iff we ship a native
 * renderer for its `format`.
 *
 * The two sides cannot drift: both read these files and neither restates the
 * list. `scripts/check-rule-registry.mjs` fails the build if a harness id or a
 * format literal reappears here (the same guard `toolRegistry.ts` has had since
 * issues #49/#53).
 *
 * Destinations use `{slug}`, the same placeholder the agent contract uses, and
 * harness ids are the agent tool ids (`piAgent`, `ohMyPi`) — one vocabulary
 * across agents and rules (issue #68). The compiler refuses a template whose
 * braces survive substitution, so a legacy `{{slug}}` becomes an error instead
 * of a literal directory.
 */

import contract from "../../../src-tauri/data/rules-tools.json";
import formats from "../../../src-tauri/data/rule-formats.json";

/** A harness destination template per scope. `null` = scope unsupported. */
export interface RuleHarnessDest {
  /** Absent when the contract declares no user-scope destination. */
  user?: string;
  /** Absent when the contract declares no project-scope destination. */
  project?: string;
}

/** The optional plugin block — read by the backend's config merge (A6). */
export interface RulePlugin {
  package: string;
  entry: string;
}

/** One harness, as authored in `rules-tools.json`. */
export interface RuleHarnessMeta {
  /** Contract id — the wire value (`rules_install` takes this, never the label). */
  id: string;
  /** Display name, e.g. "Google Antigravity". */
  label: string;
  /** Renderer key; installable iff it appears in `rule-formats.json`. */
  format: string;
  /** Install mechanism, upstream truth: "per-rule". */
  installKind: string;
  /** Path inside a harness integration tree, e.g. `{slug}/RULES.md`. */
  integrationPath: string;
  detect?: { dirs?: string[]; files?: string[] };
  frontmatter?: { required?: string[]; map?: Record<string, string> };
  extensions?: string[];
  plugin?: RulePlugin;
  dest: RuleHarnessDest;
}

/** The rule render formats this app implements natively.
 *
 *  Read from `src-tauri/data/rule-formats.json`, the same file the Rust registry
 *  embeds, so the two sides cannot drift. */
const IMPLEMENTED_FORMATS: ReadonlySet<string> = new Set(formats.formats);

// Single narrow cast at the module boundary (mirrors `toolRegistry.ts`): JSON
// imports are structurally typed by TS but not validated at runtime.
const HARNESS_MAP = (contract as unknown as { harnesses: Record<string, RuleHarnessMeta> })
  .harnesses;

/** Deterministic code-unit comparison.
 *
 *  `localeCompare` is locale-dependent and does not match Rust's `Ord for
 *  String` (byte-wise), so two platforms could order the harness list
 *  differently. Code-unit order is stable everywhere and matches Rust for the
 *  ASCII subset this contract uses.
 */
function byCodeUnit(a: string, b: string): number {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

/** All harnesses, ordered by label then id — the same order the Rust reader uses. */
export const RULE_HARNESSES: RuleHarnessMeta[] = Object.values(HARNESS_MAP).sort(
  (a, b) => byCodeUnit(a.label, b.label) || byCodeUnit(a.id, b.id),
);

const BY_ID = new Map(RULE_HARNESSES.map((harness) => [harness.id, harness]));

/** Look up a harness by contract id. `null` for an unknown id. */
export function ruleHarness(id: string): RuleHarnessMeta | null {
  return BY_ID.get(id) ?? null;
}

/** Derived installability: a native renderer exists for the harness's format. */
export function isHarnessInstallable(harness: RuleHarnessMeta): boolean {
  return IMPLEMENTED_FORMATS.has(harness.format);
}

/** The harnesses the backend can actually write — never offer anything else. */
export function installableHarnesses(): RuleHarnessMeta[] {
  return RULE_HARNESSES.filter(isHarnessInstallable);
}

/** Human label for the UI; falls back to the raw id so callers always print. */
export function harnessLabel(id: string): string {
  return BY_ID.get(id)?.label ?? id;
}
