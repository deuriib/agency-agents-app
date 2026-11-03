/**
 * Category presentation metadata for the rules corpus.
 *
 * One file — `src-tauri/data/rules-categories.json` — read here and nowhere
 * else: the Rust side never renders a category label or colour, so a second
 * reader there would be dead code. The agent side works the same way
 * (`agency-categories` on the Rust side because the corpus index carries the
 * metadata; rules carry only the slug).
 *
 * The rules corpus is upstream content and will grow, so an unknown category is
 * a normal case, not an error: {@link ruleCategoryMeta} always returns a usable
 * row and the view never drops a category it cannot dress up.
 */
import spec from "../../../src-tauri/data/rules-categories.json";

export interface RuleCategoryMeta {
  label: string;
  /** Lucide PascalCase name, bound by `resolveCategoryIcon`. */
  icon: string;
  /** Hex accent for the icon tint. */
  color: string;
  description?: string;
}

/** Neutral icon for a category the metadata does not name. */
export const FALLBACK_ICON = "Layers";
/** Neutral accent, matching the agent side's `default_division_color`. */
export const FALLBACK_COLOR = "#94A3B8";

interface CategoriesFile {
  categories: Record<string, RuleCategoryMeta>;
}

const CATEGORIES = (spec as unknown as CategoriesFile).categories;

/** `"custom-rules"` → `"Custom Rules"` — deterministic fallback label. */
function titleCase(slug: string): string {
  return slug
    .split("-")
    .map((word) => (word.length === 0 ? word : word[0].toUpperCase() + word.slice(1)))
    .join(" ");
}

/** Presentation metadata for a category, with the neutral fallback applied. */
export function ruleCategoryMeta(slug: string): RuleCategoryMeta {
  const known = CATEGORIES[slug];
  if (known !== undefined) return known;
  return {
    label: titleCase(slug),
    icon: FALLBACK_ICON,
    color: FALLBACK_COLOR,
  };
}

/** The categories this file describes — used by tests and the parity guard. */
export function declaredRuleCategories(): string[] {
  return Object.keys(CATEGORIES).sort();
}
