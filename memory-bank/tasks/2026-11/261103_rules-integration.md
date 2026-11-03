# 261103_rules-integration

## Objective

Integrate `deuriib/agency-rules` into the app (issue #29, milestone M1): a Rules
workspace that lists the rules corpus, previews the compiled output for each of the
four harnesses, installs and uninstalls per harness and scope, and tracks those
installs in the ledger alongside agents.

## Outcome

- ✅ Rust tests: 589 passing on `main` after merge (32 conformance comparisons among
  them); script tests 85; `mise run ci` green.
- ✅ **Byte-parity: 32/32** artifacts identical to a fresh upstream compiler run at
  `bde758c`, plus **88/88** on an adversarial matrix (quoting, 80-column folding,
  UTF-16 widths, astral emoji).
- ✅ Ledger: the real 176-row developer ledger parses and re-serializes byte-identically.
- ✅ i18n: 86 keys × 11 locales, enforced by `scripts/check-i18n.mjs`.
- ✅ CI: 8/8 checks green before merge (PR #61).

## Files

**New backend**

- `src-tauri/src/rules/{mod,registry,parse,validate,render,corpus,opencode,commands}.rs`
- `src-tauri/data/rules-tools.json`, `rule-formats.json`, `rules-schema.json`
- `src-tauri/resources/rules-baseline/rules/**` (bundled offline seed)
- `src-tauri/tests/rules_conformance.rs` + `tests/fixtures/rules/**`

**New frontend**

- `src/lib/stores/rules.svelte.ts`
- `src/lib/components/{Rules,RuleDetail,RulePreview,RuleInstallControls}.svelte`
- `src/lib/data/ruleRegistry.ts`

**Modified**

- `src-tauri/src/types.rs` — `Rule`, `RuleEntry`, `RuleValidation`, `RuleHarness`,
  `RuleStatus`, `InstallDomain`, `InstallRecord.domain`
- `src-tauri/src/install/mod.rs` — rule-row API and domain filtering on six surfaces
- `src-tauri/src/corpus/mod.rs` — shared extractor/downloader made reusable
- `src-tauri/src/util/fs.rs` — `flush()` before `sync_all` (see below)
- `src/lib/components/{Sidebar,CommandPalette}.svelte`, `src/routes/+page.svelte`,
  `src/lib/types.ts`, all 12 locale files, `README.md`, `CONTRIBUTING.md`

## Patterns applied

- `systemPatterns.md` §1 (corpus-copy model) — extended to a second corpus with its own
  root, index and refresh.
- `systemPatterns.md` §3 (deterministic renderer) — the four rule renderers, with
  `rendered_hash` computed over the exact bytes written.
- `registry.rs` single-source-of-truth pattern — the rule contract is read by both Rust
  (`include_str!`) and TypeScript (JSON import), with a guard that fails when either
  side restates the harness set.

## Integration points

- `src-tauri/src/lib.rs` — eight commands registered (`rules_list`, `rules_get`,
  `rules_preview`, `rules_install`, `rules_uninstall`, `rules_installs`,
  `rules_status`, `rules_refresh`).
- `src-tauri/src/install/mod.rs` — rule rows enter the same `installs.json` as agents.
- `src/routes/+page.svelte` + `Sidebar.svelte` — Cmd+7 and the palette command.

## Decisions

See `decisions.md` 2026-11-03. In short: native Rust port, second corpus, one ledger
with a discriminator, fail closed on shapes the emitter cannot reproduce.

## Defects found and fixed during review

- **`atomic_write` never flushed** before `sync_all`. `tokio::fs::File` writes
  optimistically and `sync_all` swallows the stashed error, so a full or
  quota-limited filesystem returned `Ok(())` over a truncated file and the rename made
  the truncation permanent — the ledger could be destroyed while the install reported
  success. All 20 call sites were affected.
- **`load_ledger` mapped every read error to an empty ledger**, so a permission problem
  read as "nothing installed" and the next save erased every agent and rule row.
- **Uninstall trusted ledger paths verbatim** — a hand-edited row could delete an
  arbitrary file. It now re-derives the expected destinations and refuses as a whole.
- **`opencode.json` could have entered `dests`**, which would have let uninstall delete
  a user's config. It is never a tracked destination.
- **Rule rows leaked into agent surfaces** — `antigravity`/`opencode` ids are
  byte-equal across the two schemes, so an unfiltered consumer double-counted silently.
- **UI: project scope with no folder resolved the user ledger row**, so Uninstall
  deleted the user-global rule.

## Artifacts

- PR: [#61](https://github.com/deuriib/agency-agents-app/pull/61) (merged `1ab9453`)
- Spec: `docs/rules-integration-spec.md`
- Tasks and rulings: `docs/rules-integration-tasks.md` (A1–A14, C1–C11)
- Architecture and UX contracts: `docs/rules-integration-architecture.md`,
  `docs/rules-integration-ux.md`
- Fixture provenance: `src-tauri/tests/fixtures/rules/PROVENANCE.md`
