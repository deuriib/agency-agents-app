## Active Context — Agency Agents

**State (2026-11-04)**: **PR #102 is merged** (`3ba967d`) — the updater surface no longer
offers anything it cannot deliver. `.deb` / `.rpm` builds are told they cannot self-update
(#56), release notes arrive as plain text (#57), and the retired `agencyagents.app` host is
gone from the CSP, the network panel and the locales (#47). `main` is green: `mise run ci`
passes locally (305s, 626 Rust tests, 90 frontend tests). `release-please` shipped
**v0.12.0** (`5234cca`, PR #95) just before it, so PR #102 lands unreleased on `main` and
will ship in **0.13.0**.

- **Merged in PR #102 (`3ba967d`, 3 commits, 23 files, +1304/−52)**:
  - **Bundle-type capability.** `tauri-utils` bakes the bundle type into the binary and
    `tauri-plugin-updater` dispatches on it, but a manifest can only carry one artifact
    per OS/arch — ours is the AppImage. A package install therefore handed an AppImage to
    `install_deb` and failed deterministically. `blocked_package_format()` is now a pure
    gate over all eight inputs, `inAppInstall` + `packageDownload` travel in the payload,
    and both the manual and the background path refuse before downloading anything.
  - **Honest errors.** `InvalidUpdaterFormat` and `SelfUpdateUnsupported` are their own
    variants; neither can be reported as `Network`. The plugin-error classifier was
    `#[cfg(not(test))]` and untested — it is now pure and covered.
  - **Release notes.** `strip_markdown` flattens the `release-please` body, and it runs
    *before* `MAX_NOTES_CHARS`, so a link or URL can no longer be cut in half. No
    dependency, no HTML, no sanitizer.
  - **Network posture.** `connect-src` mirrors exactly what the app reaches;
    `network.updaterDesc` deleted from all 12 locales.
  - **Evidence**: 596 → **626 Rust tests** (+30), 90 frontend tests, 10/10 CI checks green.
- **Landed since 0.9.0** (previously unrecorded here): 0.9.1 Apple identity from the
  environment (#74), 0.9.2 `{slug}` + agent tool ids for rule harnesses (#78), 0.9.3 Rules
  through the shared workspace shell (#83), 0.10.0 Antigravity app detection (#86), 0.11.0
  multi-line block scalars (#88), 0.12.0 rules install through the destinations grid (#94).
- **Still open from the #61 review**: #64 (AppHandle harness) and #65 (parity guard:
  stale vs missing) — #62, #63, #66, #68, #69 and #72 have shipped.

**Current Focus**:
1. **Publish.** Every release since 0.8.0 is a **draft with Linux assets only**. The
   blocker has not moved: signing secrets plus a local macOS run
   (`./scripts/release.sh`), then `mise run verify-updater -- --check-urls`.
2. **Native review** of the 12-locale strings — Rules, and now the three new
   `updates.*` keys added in #102.
3. **Decide on #56 option B.** Linux package installs can never self-update while the
   manifest serves a single `linux-x86_64` payload. Making them self-updatable needs
   per-installer manifests *and* a signing step for `.deb` / `.rpm`, which have no `.sig`
   today. Until then the app downloads instead — by design, not by accident.

**Open issues** (8): #29 (M1 shipped, M2 pending), #44, #64, #65, #70, #71, #76, #98.

**Known Constraints**:
- **A `.deb` / `.rpm` install cannot self-update.** Permanent until option B lands; this
  is a property of the plugin plus the one-payload-per-platform manifest, not a bug. Do
  not "fix" it by pointing `linux-x86_64` at the `.deb` — that breaks the AppImage, which
  is the artifact that works.
- `v0.12.0` and everything after 0.8.x is a **draft**; the updater cannot serve a release
  until `latest.json` exists with all five platform keys.
- macOS still needs local notarization; CI cannot produce the DMGs.
- The `agency-rules` `integrations/` tree upstream is **stale generated output** —
  regenerate before trusting a red parity run (`docs/BUILD.md`).
- Multi-line rule values fail closed (C11) rather than emitting bytes that differ from
  upstream.
- The vendored catalogs are **not verbatim**: `tools.json` diverges in five `detect.dirs`
  entries, and `agency-categories.json` uses `categories` where upstream `divisions.json`
  uses `divisions` (#72 §4 must reconcile this before reading directly from a submodule).

**Gates**:
- `mise run ci` (fmt, clippy, Rust tests, script tests, frontend tests, svelte-check,
  frontend build).
- `node scripts/check-rule-registry.mjs` and `node scripts/check-i18n.mjs`.
- `cargo test --test rules_conformance` — 32/32 byte-identical.
- `mise run verify-updater` after a release (all five platform keys).

---
## History (Obsolete)
v0.3.1 and prior contexts are now deprecated. Refer to `docs/release-notes/` for history.
