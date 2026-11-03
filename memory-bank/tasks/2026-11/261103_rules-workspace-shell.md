# 261103_rules-workspace-shell

## Objective

Issue #66. Make Rules read and behave like Agents instead of being a second,
independent workspace: one shared shell, one category model, one set of controls
— so a user who learns the Agents screen is not surprised by Rules, and the app
stops maintaining two list/detail layouts and two sets of nearly identical CSS.

## Outcome

- ✅ **`WorkspaceShell.svelte`** owns the pane row, the head slot, the scroll
  container, the resizable/persisted detail pane, the close button, `Escape`, and
  the narrow-window scrim. `AgentsWorkspace` and `Rules` both render through it;
  neither owns list/detail geometry any more.
- ✅ **`CategoryMenu.svelte`** — one dropdown for both workspaces (icon, tint,
  label, count per option, `role="menu"`, outside-click + Escape dismissal).
  Replaces the Rules native `<select>` of raw directory names.
- ✅ **`CategoryLanding.svelte`** — generic "browse by category" landing with an
  "All" row that is a real destination (it switches to the flat list), plus one
  row per category with tinted icon, label, count and optional description.
  Ordering is count desc → label → slug, so the list never reshuffles.
- ✅ **`src-tauri/data/rules-categories.json`** — label/icon/color/description for
  the shipped categories, read only by `src/lib/data/ruleCategories.ts` (the Rust
  side never renders a category label, so a second reader would be dead code).
  An unknown category renders with the neutral fallback (`Layers`, grey,
  title-cased slug) and is never dropped.
- ✅ **Install-state lens** (`All` / `Installed` / `Not installed`, with counts)
  replaces the "Installed for" select. The harness dimension stays in the detail
  pane, where every harness already has its own install control.
- ✅ **Category overview** atop a selected category's list, mirroring the Agents
  division banner (metadata only — rules have no starter prompt).
- ✅ **Row anatomy** matched: tinted category icon, name, secondary line, invalid
  badge, per-harness dots.
- ✅ **Refresh always available** in the search row (was only in the empty states).
- ✅ **No second `<h1>`**: Rules dropped its own title, Runbooks kept its subtitle
  and dropped the duplicated heading.
- ✅ Duplicated layout CSS removed: `.rv-main`, `.rv-list-pane`, `.rv-detail`,
  `.rv-toolbar`, `.rv-filter*` and the grid split are gone; `.ws/.lp-*/.detail-pane`
  live once in the shell.

## Tests

`npm test` — 58 passing (was 48):

- `Rules.test.ts` +4: the landing (counts, ordering, drill-in), the lens (counts,
  narrowing), and the always-available refresh. Existing state tests now drill
  into a category, because the landing is the default view.
- `derive.test.ts` +6: `ruleLensCounts`, `ruleCategoryTiles` ordering, the
  unknown-category fallback, and `rules-categories.json` completeness.
- `rules/corpus.rs` +1 assertion: the bundled corpus ships `base` = 6 and
  `stacks` = 2, taken from the index rather than a restated list.

## Files

**New**

- `src/lib/components/WorkspaceShell.svelte`, `CategoryMenu.svelte`,
  `CategoryLanding.svelte`
- `src/lib/data/ruleCategories.ts`, `src-tauri/data/rules-categories.json`

**Modified**

- `src/lib/components/{Rules,AgentsWorkspace,Runbooks}.svelte`
- `src/lib/rules/derive.ts` — `RuleLens`, `ruleLensCounts`, `ruleCategoryTiles`;
  `filterRules` loses the harness dimension
- `src/lib/stores/rules.svelte.ts` — `lens` replaces `harnessFilter`,
  `filterBase()` + `lensCounts()`
- `src/lib/util/categoryIcon.ts` — binds `Layers` (the neutral fallback)
- 12 locales × 3 new `rules.*` keys
- `docs/rules-integration-ux.md` — §2.1–2.4 rewritten to the delivered layout
- `src-tauri/src/rules/corpus.rs` — category counts from the index

## Deliberate deviations

- **The install action is not in the detail header.** Agents has one install
  button because an agent deploys one payload to a matrix of tools. A rule
  installs per harness × scope, so a single header button would be a different
  action wearing the same clothes. The AC's list (confirm dialog, close button,
  refresh affordance) is fully satisfied; the per-harness controls stay in the
  body.
- **Bulk actions are not implemented.** The issue marks them an explicit stretch
  item; the shell now makes them a contained follow-up (Agents' select-mode can
  move into the shell).

## Follow-ups

- Bulk select/install for rules, now that the shell exists.
- `AgentsWorkspace` still carries its own bulk-select markup; moving it into the
  shell would make the third workspace (if it comes) free.
