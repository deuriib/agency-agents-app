## Active Context — Agency Agents

**State (2026-11-03)**: **PR #61 is merged** (`1ab9453`) — the Rules integration (issue
#29, milestone M1) is on `main`. `release-please` shipped **v0.9.0** (`00b9576`,
PR #67). `main` is green: `mise run ci` passes locally on `00b9576`.

- **Merged in PR #61 (`1ab9453`, 14 commits, 105 files, +18.5k)**:
  - **Native Rust port** of the four `agency-rules` renderers (Antigravity, pi-agent,
    oh-my-pi, OpenCode). **32/32 artifacts byte-identical** to a fresh upstream
    compile at `bde758c`, plus an 88/88 adversarial matrix (quoting, folding, UTF-16
    widths, astral emoji). Golden fixtures live in
    `src-tauri/tests/fixtures/rules/` with `PROVENANCE.md`.
  - **Second managed corpus** (`rules-corpus/`, `state/rules-index.json`) with its own
    bundled baseline, tarball refresh, atomic swap and `rules_status` (distinguishes
    "not synced" from "no rules shipped") / `rules_refresh` (network-gated,
    single-flight).
  - **Ledger domain discriminator**: rule rows carry `domain: "rule"`; six agent-facing
    surfaces filter them (`installs_reconcile`, `installs_for_agent`, `tools_list`,
    `projects_list`, `loadout_export`, `loadout_import`), and agent mutations cannot
    rewrite or delete a rule row. The real 176-row ledger round-trips byte-identically.
  - **OpenCode plugin registration** (`rules/opencode.rs`) — merge-safe, atomic,
    fail-closed, `opencode.json` is never a tracked destination.
  - **Rules workspace** (Cmd+7) with all eight states, plus **86 keys × 12 locales**
    guarded by `scripts/check-i18n.mjs`.
- **Two pre-existing defects fixed on the way**:
  - `atomic_write` never called `flush()` before `sync_all`, so a full/quota-limited
    filesystem returned `Ok(())` over a **truncated file** and the rename made it
    permanent. All 20 call sites were affected; the ledger could be silently destroyed.
  - `load_ledger` mapped every read error to an empty ledger, so a permission problem
    read as "nothing installed" and the next save erased every row.

**Current Focus**:
1. **Publish v0.9.0.** The release is still a **draft** with Linux assets only
   (`.deb`, `.rpm`, AppImage + `.sig`). Missing: `latest.json`, Windows artifacts and
   the notarized macOS payload. Same blocker as 0.8.x — it needs the signing secrets
   and a local macOS release run (`./scripts/release.sh`), then
   `mise run verify-updater -- --check-urls`.
2. **Decide the follow-up order** (see below); #72 changes where the catalog lives and
   should land before #68/#66 to avoid redoing vocabulary and metadata work.
3. Native review of the 12-locale Rules strings (only English was reviewed).

**Open follow-ups from the #61 review**:

| # | Issue | Priority |
| --- | --- | --- |
| 62 | Frontend test runner + Rules workspace coverage (highest residual risk) | high |
| 64 | Tauri `AppHandle` harness — proves "visible after restart" end to end | medium |
| 66 | Rules reuse the Agents workspace shell **and** divide rules into categories | medium |
| 68 | `{slug}` + agents' tool ids (`piAgent`, not `pi-agent`) for rule harnesses | medium |
| 70 | Vasquez primary without the orchestration tool surface | medium |
| 71 | Deploy rules alongside agents from Teams and Runbooks | medium |
| 72 | `agency-agents` / `agency-rules` as pinned git submodules | medium |
| 63 | Folded block scalars, to lift the multi-line install guard (C11) | low |
| 65 | Parity guard: stale upstream artifacts vs missing ones | low |
| 69 | Detect the Antigravity app via `.gemini/antigravity` | low |

**Known Constraints**:
- `v0.9.0` is a draft; the updater cannot serve it until `latest.json` exists with all
  five platform keys.
- macOS still needs local notarization; CI cannot produce the DMGs.
- The `agency-rules` `integrations/` tree upstream is **stale generated output** —
  regenerate before trusting a red parity run (`docs/BUILD.md`).
- No frontend test runner exists: the Rules UI is covered by `svelte-check`, the
  build, and one headless DOM pass, not by automated tests (#62).
- Multi-line rule values fail closed (C11) rather than emitting bytes that differ
  from upstream.
- The vendored catalogs are **not verbatim**: `tools.json` diverges in five
  `detect.dirs` entries, and `agency-categories.json` uses `categories` where upstream
  `divisions.json` uses `divisions` (#72 §4 must reconcile this before reading
  directly from a submodule).

**Gates**:
- `mise run ci` (fmt, clippy, Rust tests, script tests, svelte-check, frontend build).
- `node scripts/check-rule-registry.mjs` and `node scripts/check-i18n.mjs`.
- `cargo test --test rules_conformance` — 32/32 byte-identical.
- `mise run verify-updater` after a release (all five platform keys).

---
## History (Obsolete)
v0.3.1 and prior contexts are now deprecated. Refer to `docs/release-notes/` for history.
