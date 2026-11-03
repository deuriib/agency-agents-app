# 261103_rule-vocabulary

## Objective

Issue #68. One contract vocabulary across agents and rules: the `{slug}`
placeholder everywhere and the `tools.json` camelCase ids (`piAgent`, `ohMyPi`)
for rule harnesses, so the app stops carrying two id schemes plus the mapping
layer that existed only because of the mismatch.

## Outcome

- ✅ `agency-rules` v0.10.0 (PR #39): harness keys and `id`s are the agent tool
  ids; destinations use `{slug}`; both substitution sites go through
  `substituteSlug`, which refuses a template whose braces survive — the legacy
  `{{slug}}` is now a loud error instead of a literally braced directory.
- ✅ `vendor/agency-rules` bumped to the tag; `rules-tools.json` regenerated
  (contract `version: 2`), so no hand-copied contract remains.
- ✅ `tool_for_harness` is an existence check — the `kebab` comparison is gone.
- ✅ Ledger compatibility: `parse_ledger` maps `pi-agent → piAgent` and
  `oh-my-pi → ohMyPi` at read time, so a v0.9 row still lists and still
  uninstalls. Pinned by `a_v0_9_ledger_row_still_uninstalls_after_the_harness_rename`.
- ✅ Conformance unchanged: the compiled bytes are identical — only ids and the
  template syntax moved (32/32, 590 Rust tests, 101 script tests).
- ✅ Fixture trees renamed to `piAgent/` and `ohMyPi/`; docs and the A2/A7
  rulings record the supersession instead of silently rewriting history.

## Files

**Upstream (`deuriib/agency-rules`, v0.10.0)**

- `tools.json` — harness ids + `{slug}`, contract version 2
- `scripts/lib/slug-template.mjs` (new), `scripts/convert.mjs`, `scripts/install.mjs`
- `tests/slug-template.test.mjs` (new), `tests/cli-filtering.test.mjs`
- `README.md`, `SECURITY.md`, `docs/ADDING-A-HARNESS.md`, `docs/ARCHITECTURE.md`

**This repo**

- `vendor/agency-rules` → v0.10.0, `src-tauri/data/rules-tools.json` (generated)
- `src-tauri/src/rules/render.rs` — `{slug}` substitution, inverted guard test
- `src-tauri/src/rules/registry.rs` — id-based `tool_for_harness`, updated tests
- `src-tauri/src/install/mod.rs` — read-time ledger alias + test hook
- `src-tauri/src/rules/{commands,opencode}.rs`, `src-tauri/src/types.rs` — ids/docs
- `src-tauri/tests/rules_conformance.rs` + fixtures renamed
- `src/lib/data/ruleRegistry.ts`, `src/lib/types.ts`
- `scripts/sync-vendor.mjs` (generated note), `scripts/check-rule-registry.test.mjs`
- `docs/rules-integration-{architecture,spec,tasks}.md`

## Patterns Applied

- One vocabulary beats an adapter: the `kebab` lookup was a mapping that existed
  only because two schemas disagreed (`ENG-12`).
- Fail closed on the old form in both repositories, so a stale template cannot
  write a path nothing reads (`ENG-10`).
- Contract parity is proven by the generator (`sync-vendor --check`) rather than
  a hand-maintained copy (`TST-04`).

## Follow-ups

- `docs/rules-integration-tasks.md`'s original task text still describes the
  double-brace form; the rulings table carries the supersession, and the text is
  historical planning material (no drive-by rewrite).
