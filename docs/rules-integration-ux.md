# Rules Integration — UI / UX Foundation (Issue #29, Milestone M1)

> **Status**: Design contract for TASK 19–23. Companion to
> `docs/rules-integration-architecture.md` (types, commands, error shapes) and
> `docs/rules-integration-tasks.md` (the authoritative task list + rulings).
>
> Every measurement below is an **existing token** from
> `src/lib/styles/tokens.css`; every structural pattern cites the component it is
> copied from. If a value is not a token, it is a `calc()` of tokens or a stated
> structural constant — never a magic number chosen for taste.
>
> **Superseded in part by issue #66.** Sections 2.1–2.4 described a second,
> independent workspace (its own `<h1>`, a CSS grid split with no resizer, and
> native `<select>` filters). The delivered Rules workspace now renders through
> the shared `WorkspaceShell` and reuses the Agents grammar; those sections were
> rewritten to match. The state model, the scope control and the preview contract
> below are unchanged.

---

## 1. Component tree and file paths

```
src/routes/+page.svelte                                    (edit — TASK 22)
└── {:else if ui.section === "rules"} <Rules />

src/lib/components/
├── Rules.svelte                       NEW  shell + header + toolbar + list pane
├── RuleDetail.svelte                  NEW  detail pane; OWNS scope + projectPath
├── RulePreview.svelte                 NEW  harness tablist + destination + bytes + copy
└── RuleInstallControls.svelte         NEW  install/uninstall + in-flight + live region
```

Reused unchanged: `EmptyState.svelte`, `LoadingState.svelte`, `Input.svelte`,
`Pill.svelte`, `Button.svelte`, `DestructiveConfirm.svelte`, `Modal.svelte`,
`toast` (`src/lib/stores/toast.svelte`), `i18n`
(`src/lib/stores/i18n.svelte`).

```
Rules.svelte
├── <header class="rv-head" data-tauri-drag-region>          ← Runbooks.svelte:108-113
│     h1  nav.rules
│     p   rules.subtitle
├── <div class="rv-toolbar">
│     Input variant="search"            rules.searchPlaceholder / rules.searchLabel
│     <select class="rv-filter">        rules.filter.harness  (options: installable harnesses)
│     <select class="rv-filter">        rules.filter.category (options: derived categories)
│     <button class="rv-clear">         rules.filter.clear    (only when a filter is active)
│     <span class="rv-count" aria-hidden> rules.count         (always: "8 rules")
├── [invalidCount > 0] <p class="rv-banner" role="status">    rules.invalid.banner.{one|many}
├── <div class="rv-body">                                     ← five states, §3
│     └── <ul class="rv-list" role="list">
│           <li><button class="rv-row" aria-current>          ← AgentsWorkspace.svelte:679-682
│                 name · category pill · invalid Pill · harness dots
└── <aside class="rv-detail">  → RuleDetail.svelte

RuleDetail.svelte
├── identity: h2 (rule.name), category Pill, description
├── <h3> rules.detail.metadata
├── <dl class="rd-fm">                     frontmatter summary (globs, alwaysApply,
│                                          trigger, match, tags, tools) — each row omitted
│                                          when the field is absent
├── validation block                       rules.validation.valid |
│                                          rules.validation.invalid + verbatim errors
├── <section class="rd-preview">
│     <h3> rules.preview.title
│     scope segment (user | project)  +  project-root row          ← OWNED HERE
│     RulePreview.svelte
├── RuleInstallControls.svelte
└── [no selection] EmptyState             rules.detail.emptyTitle / rules.detail.empty

RulePreview.svelte
├── [preview pending]                      rules.preview.loading
├── [preview failed]                       rules.preview.failed {error}
├── <div role="tablist" aria-label={rules.preview.tabsLabel}>
│     <button role="tab" aria-selected tabindex={selected ? 0 : -1}>  (installable harnesses only)
├── <div role="tabpanel" tabindex="0" aria-labelledby={tabId}>
│     <div class="rp-dest" id="rp-dest-{harness}" dir="ltr">
│           rules.preview.destination  +  <code>{dest}</code>
│     <button class="rp-copy">rules.preview.copy + rules.preview.bytes({count})</button>
│     <pre class="rp-bytes" aria-describedby="rp-dest-{harness}">{entry.bytes}</pre>
└── [invalid rule] validation block instead of tabs+bytes

RuleInstallControls.svelte
├── {#if activeHarness === "opencode"} <p class="ri-notice">   rules.install.opencodeNotice
├── <span role="status" aria-live="polite">                   install/uninstall result
└── Button (primary) install  |  Button (danger) uninstall
```

**Why four files instead of the two the tasks name.** TASK 19–21 name
`Rules.svelte` and `RuleDetail.svelte`. One `RuleDetail` carrying the body, the
frontmatter table, the validation block, a scope control, a tablist, a `<pre>`, a
copy action, a directory picker, two confirm dialogs and two live regions is a
600-line component — a God Component (ENG-14) and untestable in isolation. The
split follows the existing precedent (`InstallModal`, `DiffModal`,
`DestructiveConfirm` are all their own files). This is the file-creation
justification the repo's reuse rule requires.

---

## 2. Layout

### 2.1 Shell

`Rules.svelte` renders through `WorkspaceShell.svelte`, the same component
`AgentsWorkspace.svelte` uses (issue #66). The shell owns the geometry and the
chrome; a section owns only its content:

| Owned by the shell | Owned by the section |
| --- | --- |
| `display: flex` pane row, list pane, head slot, scroll container | the search row, the lens, the rows or the state |
| the resizable, persisted detail pane (`ui.detailPaneWidth`) | the detail body |
| the close button, `Escape` to close, the narrow-window scrim | — |

**The section renders no `<h1>`.** The shell's titlebar (`+page.svelte`) prints
the section name; a second one printed it twice. Sections may render a subtitle,
which Rules does not need (the search row is self-describing) and Runbooks keeps.

### 2.2 The two panes

```css
.ws         { display: flex; height: 100%; min-height: 0; }
.list-pane  { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
.lp-head    { flex: none; padding: var(--space-3) var(--space-4);
              border-bottom: 1px solid var(--color-border);
              display: flex; flex-direction: column; gap: var(--space-3); }
.lp-list    { flex: 1; overflow-y: auto; min-height: 0; padding: var(--space-2) var(--space-3); }
.detail-pane { flex: none; max-width: 90vw; display: flex; flex-direction: column;
               min-height: 0; background: var(--color-surface-raised);
               border-left: 1px solid var(--color-border); }
```

The detail pane is **resizable and persisted**, exactly like Agents: a
`ResizeHandle` on its left edge writes `ui.detailPaneWidth` through
`setDetailPaneWidth`. Under 860px it becomes a fixed overlay with a scrim, and
the handle hides.

### 2.3 List row

```css
.rv-row {
  width: 100%; display: flex; align-items: center; gap: var(--space-3);
  padding: var(--space-2); border-radius: var(--radius-md);
  background: transparent; color: inherit; text-align: start; cursor: pointer;
}
.rv-row:hover { background: var(--color-surface-sunken); }
.rv-row[aria-current="true"] { background: var(--color-brand-subtle); }
```

Row anatomy matches an agent row: a tinted category icon, name, a secondary line
(the rule's description), the invalid badge when validation failed, and one dot
per installable harness (filled when that rule is installed for it). Rules have no
emoji and no per-agent deploy pills — the category icon takes the emoji's place.

### 2.4 Toolbar

```css
.rv-search-row { display: flex; align-items: center; gap: var(--space-2); }
.rv-search-row :global(.wrap) { flex: 1; min-width: 0; }   /* Input's root */
.seg { display: flex; align-items: center; gap: 2px; padding: 2px;
       background: var(--color-surface-sunken); border: 1px solid var(--color-border);
       border-radius: var(--radius-md); }
```

The row holds, in order: the **category dropdown** (`CategoryMenu.svelte`, the
same component and the same icon + tint + count options Agents uses), the search
input, a clear-filters button when anything narrows the list, the result count, and
a **refresh** button that is always available.

Below it, a **segmented install-state lens** (`All`, `Installed`, `Not installed`)
with counts, mirroring the Agents `LENSES`. It replaces the M1 "Installed for"
`<select>`: the neutral rule schema has no per-harness target, so that control could
only answer from the ledger — which is exactly what the per-rule lens now does. The
harness dimension stays in the detail pane, where each harness has its own install
control.

Categories come from the corpus, dressed by `src-tauri/data/rules-categories.json`
(label, Lucide icon, hex accent, optional description). A category with no entry
renders with the neutral fallback (`Layers`, grey, title-cased slug) and is never
dropped.

### 2.5 Detail typography and the preview block

```css
.rd-title   { font-size: var(--text-h1); font-weight: var(--fw-semibold);
              color: var(--color-text-primary); }
.rd-desc    { font-size: var(--text-body-sm); color: var(--color-text-secondary);
              line-height: var(--lh-normal); }
.rd-fm      { display: grid; grid-template-columns: max-content 1fr;
              gap: var(--space-1) var(--space-3); margin: 0;
              font-size: var(--text-body-sm); }
.rd-fm dt   { color: var(--color-text-muted); }
.rd-fm dd   { margin: 0; color: var(--color-text-primary);
              font-family: var(--font-mono); font-size: var(--text-mono); }

.rp-dest { display: flex; align-items: center; gap: var(--space-2);
           font-size: var(--text-caption); color: var(--color-text-muted);
           font-family: var(--font-mono);
           /* paths are LTR even in an RTL UI — see §6 */
           direction: ltr; unicode-bidi: isolate; word-break: break-all; }
.rp-bytes {
  margin: 0; padding: var(--space-3);
  max-height: 420px; overflow: auto;
  border-radius: var(--radius-md);
  background: var(--color-console-bg); color: var(--color-console-fg);
  font-family: var(--font-mono); font-size: var(--text-mono);
  line-height: var(--lh-mono); tab-size: 2;
  /* the bytes are the product: never wrap them */
  white-space: pre;
}
```

`background: var(--color-console-bg)` / `color: var(--color-console-fg)` is the
existing console pair (tokens.css:58-59, 122-123) — high contrast in both themes
and visually distinct from every other surface, which matters because this block
is *config file content*, not prose.

---

## 3. Every UI state

Seven states. They are not interchangeable; five of them are the exact defects the
Tasks call out. A single `<div>` that collapses them fails the tasks.

| # | State | Trigger | Rendered as | Copy keys |
| --- | --- | --- | --- | --- |
| 1 | **Loading** | `!rules.loaded \|\| rules.loading` | `LoadingState` (`rows={7}`) — skeleton rows, `role="status"`, `aria-live="polite"` (`LoadingState.svelte:10-13`). Detail pane keeps its `rules.detail.empty` placeholder. | `common.loading` (existing key) |
| 2 | **Error** | `rules.error !== null` | Full-pane `EmptyState` + `TriangleAlertIcon size={44}` + **verbatim** `rules.error` + retry button. Mirrors `Runbooks.svelte:118-125`. Never degrades to an empty state (`decisions.md` 2026-11-02 / issue #58). | `rules.loadFailedTitle`, `rules.loadFailed`, `rules.retry` |
| 3 | **Empty — not synced** | `status.emptyReason === "notSynced"` | `EmptyState` + `DownloadIcon` + **CTA: sync**. Copy states the rules catalog has not been downloaded yet. | `rules.empty.notSyncedTitle`, `rules.empty.notSynced`, `rules.empty.notSyncedCta` |
| 4 | **Empty — no rules shipped** | `status.emptyReason === "noRules"` | `EmptyState` + `ScrollTextIcon` + **CTA: refresh**. Different icon, different title, different sentence: the catalog *is* synced and genuinely carries no rules. | `rules.empty.noRulesTitle`, `rules.empty.noRules`, `rules.empty.noRulesCta` |
| 5 | **Partial — some rules invalid** | `list.length > 0 && invalidCount > 0` | List renders normally; every invalid row carries a `Pill tone="danger"` with `rules.invalid.badge`; a banner above the list says how many cannot be installed. Invalid rules are **never hidden** (TASK 19, spec §10.4). | `rules.invalid.banner.one`, `rules.invalid.banner.many` |
| 6 | **Filtered to zero** | list non-empty, filters compose to zero | Inline block **inside the list pane** (not a full-pane `EmptyState`): one line + a "clear filters" button. Distinct from #3/#4 because the corpus is fine — the query is the problem. | `rules.filtered.none`, `rules.filtered.clear` |
| 7 | **Installed / not installed** | per `(rule, harness, scope)` in `rules.installations` | Install control shows "Installed" + an uninstall button, or the install button. State comes from the ledger re-read, never from local mutation (spec §10.6). | `rules.install.installed`, `rules.install.notInstalled`, `rules.uninstall.action` |

State 3 vs 4 is the load-bearing distinction (spec §8, `decisions.md`
2026-11-02): same visual language, different sentence, different CTA, different
icon. A dev who renders one `list.length === 0` branch fails the task.

The banner in state 5 uses `--color-warning-strong` for its **text**, never
`--color-warning` (which is 2.9:1 on light surfaces — tokens.css:36).

---

## 4. The compiled-preview interaction

### 4.1 Scope control (owned by `RuleDetail`, not `RulePreview`)

There is exactly **one** scope control in the pane, and it drives both the preview
and the install action. Two scope controls would let the user preview user-scope
bytes while installing to a project (ENG-17: no duplicated state).

```
RuleDetail owns:  let scope = $state<"user" | "project">("user");
                  let projectPath = $state<string | null>(null);
                  let harness  = $state<string | null>(null);   // first installable
```

The scope segment sits immediately left of the harness tablist (TASK 20: "Scope
switch sits with the harness switch"). Only scopes the active harness declares are
offered — derived from `ruleHarness(id).dest.user/project` in `ruleRegistry.ts`,
never a literal list. All four shipped harnesses declare both, but the UI must not
assume it. Label: `rules.scope.label`; options `rules.scope.user` /
`rules.scope.project`.

Switching to `project` with no `projectPath` does **not** clear the preview: the
preview panel shows `rules.scope.needsRoot` plus a `rules.scope.chooseProject`
button, and no path, no bytes, no copy action (spec §7: never show a user path
while project scope is selected).

### 4.2 Harness switcher (real tablist)

```svelte
<div role="tablist" aria-label={i18n.t("rules.preview.tabsLabel")}>
  {#each installableHarnesses() as h (h.id)}
    <button role="tab"
            id="rp-tab-{h.id}"
            aria-selected={harness === h.id}
            aria-controls="rp-panel-{h.id}"
            tabindex={harness === h.id ? 0 : -1}
            onclick={() => (harness = h.id)}>{harnessLabel(h.id)}</button>
  {/each}
</div>
<div role="tabpanel" id="rp-panel-{harness}" aria-labelledby="rp-tab-{harness}" tabindex="0">
```

Roving tabindex + `ArrowLeft`/`ArrowRight`/`Home`/`End` move selection and focus
(`ArrowRight`/`ArrowLeft` swap in an RTL locale — read `document.documentElement.dir`).
Options are **only** the harnesses `installableHarnesses()` returns; an unknown
harness never appears (spec §8).

### 4.3 Destination path + byte affordance

- The destination is printed above the bytes in `--font-mono` /
  `--text-caption`, with `direction: ltr; unicode-bidi: isolate`. A path must never
  be bidi-reordered — in `fa` a raw path renders reversed and the user copies the
  wrong thing.
- The `<pre>` gets `id="rp-bytes"` and `aria-describedby` pointing at the path
  element, so a screen reader announces the destination before the content
  (TASK 20 requires this link).
- **Byte-level copy**:
  - The button label carries the exact byte count: `rules.preview.copy` +
    `rules.preview.bytes` (`{count}` = `entry.bytes.length` UTF-8 bytes… use
    `new TextEncoder().encode(entry.bytes).length`, not `.length`, since rules
    contain em dashes and ✅/❌ glyphs).
  - `navigator.clipboard.writeText(entry.bytes)` — **the string as returned**,
    with its trailing newline, no `.trim()`, no re-wrapping (TASK 20).
  - Success → `toast.success(i18n.t("rules.preview.copied"))` **and** a
    `role="status" aria-live="polite"` line that says the same thing (the toast is
    transient; the live region is the a11y contract).
  - Failure → `toast.error(i18n.t("rules.preview.copyFailed"), errorText(e))`,
    mirroring `Runbooks.svelte:97-104`.
- The `<pre>` renders raw text. No markdown renderer, no syntax highlighting, no
  `{@html}` (spec forbids a renderer, and any highlighter that alters characters
  breaks the copy path).

### 4.4 Invalid rule

When `rule.validation.valid === false`:

- The preview section is replaced by a validation block: heading
  `rules.preview.invalidTitle`, then **each** string from
  `rule.validation.errors` in its own `<li>` inside `<ul>`, verbatim, in
  `--font-mono` / `--text-mono`, no truncation, no summarising (spec §8).- The tablist is **not rendered at all** — not rendered-and-disabled. There are no
  bytes to show, `rules_preview` refuses invalid rules (TASK 15), and an empty
  tabpanel is worse than no panel.
- The install button is disabled with `rules.install.reasonInvalid` as its stated
  reason (TASK 21: "disabled-without-a-reason is a defect").
- The footer of the block states `rules.preview.invalid` ("No files will be
  written for this rule") so the failure is unambiguous.

`rule.validation.valid === true` renders a calm pass line
(`rules.validation.valid` with `Pill tone="success"`), not a banner — a passing
validation is not news.

---

## 5. Install / uninstall interaction

### 5.1 Confirmation copy

Install is a **write to the user's config tree**, so it confirms first, naming the
resolved path:

| Case | Key | Interpolations |
| --- | --- | --- |
| user scope | `rules.install.confirmUser` | `{rule}`, `{harness}`, `{dest}` |
| project scope | `rules.install.confirmProject` | `{rule}`, `{harness}`, `{project}`, `{dest}` |

`{dest}` is the **absolute** path from `RulePreviewEntry.dest`; `{project}` is the
picked directory's basename. Use `DestructiveConfirm.svelte` with
`confirmVariant="primary"` (it is additive, per that component's own doc comment)
and `defaultFocus="cancel"` (the component's default) so an accidental Enter is
the safe path.

### 5.2 OpenCode plugin notice

Shown when the active harness is `opencode`, above the install button, always
(not only on first install) — the app cannot know client-side whether the entry is
already there, and a false "nothing else will change" is worse than a conditional
"if missing":

> `rules.install.opencodeNotice` — "OpenCode rules load through the **{plugin}**
> plugin. If it is not registered yet, the app adds it to `opencode.json`."

`{plugin}` is `ruleHarness("opencode").plugin.package` read from
`ruleRegistry.ts`. **Never hardcode `opencode-rules`** in the component (architecture
§8 / A6).

### 5.3 Project root picker

- Reuses `@tauri-apps/plugin-dialog`'s `open({ directory: true, multiple: false,
  title })` — the same import and call shape as `ToolsView.svelte:32` and
  `CatalogFirstRun.svelte:16`.
- The chosen path is displayed in `--font-mono` with `dir="ltr"`, plus a
  `rules.scope.changeProject` affordance. Row label: `rules.scope.projectRoot`;
  empty state: `rules.scope.projectRootNone`; picker title:
  `rules.scope.chooseProjectTitle`.
- It is **local component state only**: no `projects_add`, no
  `ui.setSection("projects")`, no `localStorage` write. TASK 21 is explicit — a
  rules install must not create a Projects-registry entry (`projects_list` must be
  byte-identical before and after; that is an acceptance assertion).
- With scope = project and no path: the install button is disabled with
  `rules.install.reasonNoProjectRoot`.

### 5.4 Uninstall

- Available for every `(rule, harness, scope)` present in `rules.installations`
  (the ledger re-read). A row can offer uninstall while the rule file is already
  gone — that is the `Removed`-equivalent case and uninstall must still clean the
  ledger row (TASK 17).
- Confirmation via `DestructiveConfirm.svelte`, `confirmVariant="danger"`:

  > `rules.uninstall.confirmTitle` — "Remove {rule} for {harness}?"
  >
  > `rules.uninstall.confirm` — "The file(s) this app wrote are deleted and the
  > install record is removed."
  >
  > `rules.uninstall.pluginKept` (only when harness is `opencode`) — "The
  > **{plugin}** entry in `opencode.json` is kept — other rules may depend on it."

  The plugin-kept sentence is **mandatory** for the opencode harness: A6 forbids
  removing the entry, and a user who expects a clean uninstall must be told.

### 5.5 In-flight and result

- While a mutation is pending: the control group gets `aria-busy="true"`, every
  button in it is `disabled`, the acting button uses `Button`'s `loading` prop
  (spinner + `rules.install.busy` / `rules.uninstall.busy`). Double submit is
  impossible (TASK 21).
- Result: `role="status" aria-live="polite"` element inside
  `RuleInstallControls.svelte` with `rules.install.success` /
  `rules.uninstall.success` / `rules.install.failed` (the latter carries
  `{error}` = `errorText(e)` verbatim). **Plus** the matching toast. Both: the
  toast is for sighted glanceability, the live region is the contract.
- After success the component does **not** flip a local `installed` flag: it
  awaits `rules.installations` being re-read (`rules_installs`) and re-derives.
  A failure leaves the previous install state untouched and displays the error;
  it never claims success (TASK 21).

---

## 6. Accessibility contract

**Roles**

| Element | Role / attribute |
| --- | --- |
| Rule list | `<ul role="list">`; each row is a `<li>` whose only child is a `<button>` (never a clickable `<div>`) |
| Selected row | `aria-current="true"` on the row `<button>` |
| Row accessible name | composed: rule name + category + invalid state + install presence. `rules.list.itemLabel` provides the format; the invalid/installed fragments are their own keys so translations can reorder them |
| Harness switcher | `role="tablist"` + `aria-label`; tabs `role="tab"` / `aria-selected` / `aria-controls` / roving `tabindex`; panel `role="tabpanel"` / `aria-labelledby` / `tabindex="0"` |
| Scope segment | `role="radiogroup"` + `aria-label`, each option `role="radio"` + `aria-checked` |
| Filters | native `<select>` with a real `<label for>`; the search `Input` gets `ariaLabel` |
| List scroll region | when the list gets its own scroll container, wrap in `role="region"` + `aria-label={rules.list.regionLabel}` (project convention: `AgentsWorkspace.svelte:482` uses a labelled `<aside>`) |
| Banners | invalid-count banner `role="status"`; load failure `role="alert"` |
| Install result | `role="status" aria-live="polite"` (never `assertive` — a failed install is not an emergency) |

**Labels.** No icon-only button without `ariaLabel`. The invalid badge is not
`aria-hidden` — its meaning is inside its text (`rules.invalid.badge`), and the
row's own name does not otherwise convey it.

**Focus order** (top to bottom, no traps):

```
search → harness filter → category filter → [clear filters] → [invalid banner link]
→ list (row 1 … row n) → scope segment → [project row + change] → harness tablist
→ [copy] → bytes panel (tabindex=0, so the content is reachable and scrollable by keyboard)
→ [install | uninstall]
```

**Keyboard**

| Key | Where | Action |
| --- | --- | --- |
| `/` | anywhere not typing | focus the search field — already global (`+page.svelte:119-126`); it finds the first `input[type="text"\|"search"]` in DOM order, which is the rules search box when this section is mounted |
| `ArrowDown` / `ArrowUp` | list row focused | move selection to next/previous row, focus follows |
| `Enter` / `Space` | row | select |
| `ArrowLeft` / `ArrowRight` | tablist | previous/next harness (mirrored in RTL) |
| `Home` / `End` | tablist | first/last harness |
| `Tab` | bytes panel | leaves normally; the panel is one stop, not a trap |
| `Escape` | confirms/modals | closes the modal, handled by `Modal.svelte` |

Note: `Escape` is **not** bound to "clear filters" — `+page.svelte:114-117` owns
Escape for settings/palette and the rules view must not intercept it.

**Live regions.** Three, each fire-once per event: install result, preview copy,
invalid-rule errors. Do not put a live region around the list (it would read the
whole catalog on every keystroke).

**Contrast (WCAG 2.1 AA, ENG-23).**

- Body and secondary text: `--color-text-primary` / `--color-text-secondary`.
- Muted text: `--color-text-muted` — certified ≈5.7:1 on `--color-surface` in
  light and ≈? on dark; tokens.css:28 records the light math, and the dark value
  is the paired token. Do not introduce a third grey.
- Warning text: `--color-warning-strong`, never `--color-warning` (2.9:1 failure,
  documented at tokens.css:36).
- Badges: use `Pill` with `tone="danger"` / `tone="success"` — those pair with
  `--color-danger-on-subtle` / `--color-success-on-subtle`, both certified in
  tokens.css:46-51 and 110-115. **Never** build a badge with
  `color: var(--color-danger)` on a subtle background.
- Focus: the global `:focus-visible` in `reset.css:61-66` supplies the outline and
  ring. Do not override `outline` on these new elements — only `outline-offset`
  if a rounded host needs it.
- The console block's `--color-console-fg` on `--color-console-bg` is high
  contrast in both themes (tokens.css:57-60, 121-124).

**RTL.** `fa` flips the document direction (`i18n.svelte.ts:38-45`). Therefore:
`text-align: start`, `margin-inline`-style spacing where a side is chosen, and
`direction: ltr; unicode-bidi: isolate` on **every** path and on the bytes `<pre>`.
The bytes `<pre>` must stay LTR even in `fa` — YAML/HTML content is not
direction-relative.

---

## 7. Complete i18n key namespace

Every key below goes into `src/lib/i18n/locales/en.ts` (source of truth for
`MessageKey`, `en.ts:849-851`) and into all eleven other locale files:
`de`, `es`, `fa`, `fr`, `ja`, `ko`, `pt-BR`, `ru`, `tr`, `zh-CN`, `zh-TW`
(`messages.ts:3-14`). TASK 23 requires all twelve, named individually.

Conventions that the guard enforces and a dev must not break:

- Flat dotted keys, `rules.*`, plus exactly `nav.rules` and `palette.openRules`.
- Interpolation placeholders must match `/\{(\w+)\}/` (`i18n.svelte.ts:43-46`) —
  letters, digits, underscore only. **No hyphens, no spaces.**
- Plurals use the existing `.one` / `.many` pair read through
  `i18n.count(n, one, many)` (`i18n.svelte.ts:66-68`).
- No string concatenation at call sites (ENG-30).
- Every key below is consumed by exactly one element named in §1–§5. If a key ends
  up unused, delete it rather than shipping a dead string — the same "no dead
  fields" rule TASK 18 applies to the store.

### Navigation (2)

```
nav.rules
palette.openRules
```

### View shell (4)

```
rules.subtitle
rules.count                      {count}
rules.retry
rules.list.regionLabel
```

### Load failure (2)

```
rules.loadFailedTitle
rules.loadFailed                 {error}
```

### Search, filters, filtering (9)

```
rules.searchPlaceholder
rules.searchLabel
rules.filter.harness
rules.filter.harnessAll
rules.filter.category
rules.filter.categoryAll
rules.filter.clear
rules.filtered.none
rules.filtered.clear
```

### List rows (2)

```
rules.list.itemLabel             {name} {category}
rules.list.invalidLabel
```

### Empty states (6)

```
rules.empty.notSyncedTitle
rules.empty.notSynced
rules.empty.notSyncedCta
rules.empty.noRulesTitle
rules.empty.noRules
rules.empty.noRulesCta
```

### Invalid rules (6)

```
rules.invalid.banner.one         {count}
rules.invalid.banner.many        {count}
rules.invalid.badge
rules.invalid.badgeTitle
rules.invalid.sectionTitle
rules.invalid.errorsHeader
```

### Detail pane (3)

```
rules.detail.emptyTitle
rules.detail.empty
rules.detail.metadata
```

### Frontmatter summary (6)

```
rules.frontmatter.globs
rules.frontmatter.alwaysApply
rules.frontmatter.trigger
rules.frontmatter.match
rules.frontmatter.tags
rules.frontmatter.tools
```

### Validation (2)

```
rules.validation.valid
rules.validation.invalid
```

### Scope selector (9)

```
rules.scope.label
rules.scope.user
rules.scope.project
rules.scope.needsRoot
rules.scope.projectRoot
rules.scope.projectRootNone
rules.scope.chooseProject
rules.scope.chooseProjectTitle
rules.scope.changeProject
```

### Compiled preview (11)

```
rules.preview.title
rules.preview.tabsLabel
rules.preview.destination
rules.preview.bytes              {count}
rules.preview.copy
rules.preview.copied
rules.preview.copyFailed
rules.preview.loading
rules.preview.failed             {error}
rules.preview.invalidTitle
rules.preview.invalid
```

### Install (13)

```
rules.install.action
rules.install.busy
rules.install.confirmTitle
rules.install.confirmUser         {rule} {harness} {dest}
rules.install.confirmProject      {rule} {harness} {project} {dest}
rules.install.success             {rule} {harness}
rules.install.failed              {rule} {harness} {error}
rules.install.reasonInvalid
rules.install.reasonNotInstallable
rules.install.reasonNoProjectRoot
rules.install.opencodeNotice      {plugin}
rules.install.installed
rules.install.notInstalled
```

### Uninstall (7)

```
rules.uninstall.action
rules.uninstall.busy
rules.uninstall.confirmTitle      {rule} {harness}
rules.uninstall.confirm
rules.uninstall.pluginKept        {plugin}
rules.uninstall.success
rules.uninstall.failed            {error}
```

**Total: 82 new keys** — 2 nav + 80 `rules.*`. Count them in each of the twelve
files; "eleven other locales" is not the same as naming them.

`scripts/check-i18n.mjs` (TASK 23) asserts, for the new namespaces only
(`rules.` / `nav.rules` / `palette.openRules`), that every `en.ts` key exists in
each of the eleven other files — scoped deliberately, because the locale files are
`Partial<Messages>` (`messages.ts:47-60`) and already have unrelated gaps (A12).

---

## 8. Do not

Each of these breaks the design in a way that will not show up in `mise run ci`.

1. **Do not hardcode a harness id, label, or destination** anywhere in `.svelte` or
   `.ts`. Read `ruleRegistry.ts` (`RULE_HARNESSES`, `installableHarnesses()`,
   `harnessLabel()`). A literal list is the exact defect behind issues #49 and #53.
2. **Do not hardcode `opencode-rules`** — interpolate `plugin.package` /
   `plugin.entry` from the contract.
3. **Do not add a second scope control.** One scope, owned by `RuleDetail`, consumed
   by both the preview and the install action.
4. **Do not reuse the agent `Bot` icon** for Rules. `ScrollText` is the
   recommendation (`BookMarked` / `Ruler` are acceptable); add the one-line comment
   next to the nav entry saying it is deliberately distinct from Agents.
5. **Do not import `src/lib/stores/install.svelte.ts`** — that journal is the *agent*
   activity log and rule installs must not pollute it (TASK 18).
6. **Do not optimistically set `installed = true`.** Re-read `rules_installs` and
   derive from the ledger (spec §10.6).
7. **Do not trim, re-wrap, or highlight the preview bytes** before copying. Copy
   the returned string verbatim, trailing newline included.
8. **Do not render markdown** in the neutral body — no `marked`, no `mdsvex`, no
   `{@html}`. A monospace `<pre>` is in scope; a renderer is not (spec §7, TASK 20).
9. **Do not call `projects_add` or otherwise register the picked project.** Project
   scope for rules is a path, not a Projects entry (TASK 21).
10. **Do not inline-style anything except dynamic values.** `style="width: {n}px"`
    is the only sanctioned form (`Sidebar.svelte:75`, `AgentsWorkspace.svelte:482`);
    everything else is a token in a `<style>` block.
11. **Do not use `--color-warning` for text.** `--color-warning-strong` exists for
    exactly this (tokens.css:36).
12. **Do not render a rule as installed for *all* harnesses** because one harness is
    installed. Install state is per `(rule, harness, scope)`; a project-scope install
    and a user-scope install are two independent rows.
13. **Do not disable a control without stating why.** Every disabled install button
    carries one of `rules.install.reason*`.
14. **Do not add a rule key under `agents.*` or a rule string outside `rules.*`.**
    The i18n guard is scoped to the new namespaces; a misplaced key escapes it and
    ships English-only.
15. **Do not put a live region around the whole list.** It re-announces the catalog
    on every keystroke.
16. **Do not bind `Escape`.** `+page.svelte:114-117` owns it.
17. **Do not reuse the agent renderers' frontmatter template.** Those emit
    `---\n…\n---\n{body}\n`; rules need the blank line (§5.5 of the architecture
    doc). This is the single most common way to fail all 28 conformance checks.
18. **Do not add a rule counter to `AgencyDashboard.svelte`, a Rules filter to
    `Projects.svelte`, or a rule badge to the sidebar footer.** All three are M2
    (`tasks.md:38-56`).
