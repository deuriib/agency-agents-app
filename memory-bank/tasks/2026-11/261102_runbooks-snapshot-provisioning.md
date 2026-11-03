# Runbooks pillar never populated by app-provisioned catalogs (#58)

## Objective

Fix the report that the Runbooks pillar stayed on "Sync your catalog to unlock
Runbooks" even though the catalog source (`deuriib/agency-agents`) ships
`strategy/runbooks.json`. Root cause: the app's own snapshot provisioning created
a catalog with **only division directories** — no `strategy/`, no
`divisions.json`, no `scripts/` — and then treated it as healthy forever.

## Root Cause

The catalog removed `scripts/convert.sh` on 2026-10-26
(`refactor(repo): remove legacy Bash scripts`), and the snapshot pipeline was
keyed on that file:

1. `categories_from_tarball` parsed `AGENT_DIRS` out of `scripts/convert.sh` →
   file gone → `None` → fallback to the hardcoded `bundled_division_slugs`.
2. `extract_categories` kept an allow-list: `<category>/*.md` plus
   `scripts/convert.sh`. Everything else in the tarball — `strategy/`,
   `divisions.json`, `scripts/` — was dropped.
3. The division-only result passed `looks_like_catalog`, so `provision_managed`
   short-circuited and never repaired it.
4. `runbooks_list` returned `[]` for the absent manifest, and the store swallowed
   every error, so the UI showed a misleading "sync to unlock" CTA.

Verified against the live tarball: it contains `strategy/runbooks.json` and
`divisions.json`, and no `scripts/convert.sh`. A user's provisioned
`<home>/.agency-agents` contained 18 division dirs and nothing else.

## Outcome

- `-` `categories_from_tarball` prefers `divisions.json` (keys-only parse), with
  `convert.sh` as a legacy fallback.
- `extract_categories` → `extract_catalog`: preserves `strategy/**`,
  `divisions.json`, `scripts/**` and nested agent paths; rebuilds every
  destination from validated path components (`archive_rel_components`).
- `provision_managed` repairs a stale snapshot (`snapshot_is_stale` →
  `reextract_snapshot`) instead of trusting `looks_like_catalog` alone.
- `read_runbooks` separates "manifest absent" (empty, bundled) from
  "manifest unreadable" (typed error), and the Runbooks view renders the error
  with a retry action.
- `seed_from_baseline` carries `divisions.json` + `strategy/` when a baseline
  ships them.
- Tests: 373 passed (+6), 5 integration; `fmt`, `clippy -D warnings`,
  `svelte-check` 0/0, 67 script tests, frontend build.
- Real-tarball probe (throwaway, removed): 18 divisions, 298 agents, 17 runbooks,
  `divisions.json` and `scripts/` present.

## Files Modified

- `src-tauri/src/corpus/mod.rs` — discovery, extractor, provisioning repair,
  `read_runbooks`, tests.
- `src/lib/stores/runbooks.svelte.ts` — `error` state instead of a silent `[]`.
- `src/lib/components/Runbooks.svelte` — error empty-state + retry.
- `src/lib/i18n/locales/{en,es}.ts` — `runbooks.loadFailedTitle`,
  `runbooks.loadFailed`, `runbooks.retry`.
- `README.md` — what a catalog carries and the `strategy/` dependency.

## Patterns Applied

- `systemPatterns.md` catalog source of truth: the repo is the catalog; the app
  mirrors it rather than curating a subset.
- Cold-path synchronous recursion (`copy_repo_meta`) is acceptable for the tiny,
  trusted baseline seed; hot paths stay async.

## Follow-ups

- Bundled offline baseline still has no `strategy/`; the next baseline
  re-vendor now seeds it with no code change. Tracked as content work, not a bug.
- The manifest's `doc` field is not rendered yet (staged branch
  `feat/runbooks-doc-render-staged`).
