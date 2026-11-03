# 261103_pinned-catalog-submodules

## Objective

Issue #72. Replace the hand-vendored copies of `agency-agents` and `agency-rules` with
pinned git submodules, and derive every embedded contract and baseline from them through
one generator plus one verifier, so the catalog relationship is explicit, testable and
impossible to drift silently.

## Outcome

- ✅ `vendor/agency-agents` pinned to **v1.6.2**, `vendor/agency-rules` to **v0.9.0**
  (a tag cut for this work: the app already shipped rule artifacts from unreleased
  commits).
- ✅ `scripts/sync-vendor.mjs` generates `data/tools.json`, `data/rules-tools.json`,
  `resources/corpus-baseline/` and `resources/rules-baseline/`; `--check` fails on
  stale, missing or extra files and runs in `mise run ci` and PR CI (`vendor` job).
- ✅ Drift reconciled: five `detect.dirs` divergences are now an explicit, reasoned
  overlay (`data/tools-overlay.json`); `data/agency-categories.json` is deleted and the
  Rust floor embeds the catalog's own `divisions.json`.
- ✅ Baseline refreshed from v1.6.2: 283 → 334 files (297 agents indexed). Seeding is now
  recursive, so nested divisions (`game-development/unity/…`) survive a first run.
- ✅ Fail fast: `build.rs` stops before `tauri_build` with the submodule init command.
- ✅ All 12 `actions/checkout` sites fetch submodules; parity roots default to `vendor/`.
- ✅ Rust 589 tests, script tests 102, `svelte-check` and the production build green.
- ⚠️ Pinning surfaced an upstream defect: `finance/finance-profit-first-strategist.md`
  has frontmatter that is invalid YAML, so 297 of 298 files index. Fix proposed in
  `deuriib/agency-agents#47`; the corpus test names it until the bump lands.
- ⚠️ v1.6.2 predates the catalog's move to `python -m agency_agents`, so the bash parity
  test reports "skipping" instead of proving bytes. Manifest-based oracle: #76.

## Files

**New**

- `.gitmodules`, `vendor/agency-agents`, `vendor/agency-rules`
- `scripts/sync-vendor.mjs`, `scripts/sync-vendor.test.mjs`
- `src-tauri/data/tools-overlay.json`

**Deleted**

- `src-tauri/data/agency-categories.json` (replaced by the submodule's `divisions.json`)

**Modified**

- `src-tauri/build.rs` — submodule preflight
- `src-tauri/src/corpus/mod.rs` — embedded `divisions.json`, recursive seeding, pinned
  baseline counts
- `src-tauri/src/render/mod.rs`, `src-tauri/tests/rules_conformance.rs` — parity roots
  default to `vendor/`
- `.mise.toml` — `sync-vendor`, `check-vendor`, `submodules`; `ci` includes the check
- `.github/workflows/*` — `submodules: recursive` on every checkout + the `vendor` job
- `.gitleaks.toml` — `vendor/` allowlisted (third-party, scanned upstream)
- `CONTRIBUTING.md`, `docs/BUILD.md`, `README.md`, `memory-bank/decisions.md`

## Patterns Applied

- One generator + one verifier instead of a runtime merge implemented in three
  languages (`ENG-06`, `ENG-12`).
- A closed overlay that rejects dead and unknown entries — the divergence list cannot
  rot (`TST-04`: invariants, not counts).
- Generated artifacts are reproducible byte-for-byte; the test regenerates into a
  scratch tree and compares (`ENG-10`).

## Architectural Decisions

- Submodules over a lockfile/tarball or `git subtree`; generated copies over read-through
  so every consumer (Rust `include_str!`, TS JSON import, Node guards) keeps one file.
- Detection roots stay an app overlay rather than an upstream change, so the pin is not
  coupled to an `agency-agents` release.
- See `memory-bank/decisions.md#2026-11-03--the-catalog-and-rule-contracts-are-pinned-submodules-generated-into-the-tree`.

## Follow-ups

- #76 — replace the bash parity oracle with `convert-outputs.sha256`.
- `deuriib/agency-agents#47` — quote the Profit First description (blocked: the repo's
  required check `ci/required` is not produced for branch PRs, so no PR can merge).
- #68 now becomes an upstream change in `agency-rules` plus a `mise run sync-vendor`.
