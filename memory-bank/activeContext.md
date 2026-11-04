## Active Context — Agency Agents

**State (2026-11-04)**: **PR #102 is merged** (`3ba967d`) — the updater surface no longer
offers anything it cannot deliver. `.deb` / `.rpm` builds are told they cannot self-update
(#56), release notes arrive as plain text (#57), and the retired `agencyagents.app` host is
gone from the CSP, the network panel and the locales (#47). It shipped as **v0.12.1**
(`e924e09`, published 2026-11-04 03:36Z). `main` is green: `mise run ci` passes locally
(305s, 626 Rust tests, 90 frontend tests).

> **Correction (2026-11-04).** The first version of this file said PR #102 would ship in
> `0.13.0` and repeated "every release since 0.8.0 is a draft with Linux assets only".
> Both were wrong: `gh release list` shows **0.8.1 → 0.12.1 all published**, and `latest.json`
> is live on `releases/latest/download`. Measured with `mise run verify-updater` on
> v0.12.1: **3 of 5 platform keys OK** (`linux-x86_64`, `windows-x86_64`, `windows-aarch64`),
> **`darwin-aarch64` and `darwin-x86_64` missing** → Result: **FAIL**. macOS is the only
> gap; the publish blocker is much narrower than it had been recorded.

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
1. **Publish the macOS payload.** This is the whole remaining release gap. Linux and
   Windows have been live since 0.8.1; only `darwin-aarch64` / `darwin-x86_64` are absent,
   which is why `mise run verify-updater` still returns **FAIL**. CI cannot notarize, so it
   needs a local `./scripts/release.sh` on the Mac, then
   `mise run verify-updater -- --check-urls`.
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
- **macOS has no updater payload.** `latest.json` carries three of five platform keys;
  `darwin-aarch64` / `darwin-x86_64` are missing because notarization is local, so macOS
  cannot install an update and `mise run verify-updater` fails. **Releases themselves are
  published** (0.8.1 → 0.12.1), so "the release is a draft" is no longer the explanation for
  anything — check `gh release list` before claiming it.
- macOS still needs local notarization; CI cannot produce the DMGs.
- **A live manifest confirms the #56 premise.** v0.12.1's `linux-x86_64` entry points at
  `Agency.Agents_0.12.1_amd64.AppImage.tar.gz`, while the same release attaches a `.deb` and
  an `.rpm` with no `.sig`. Both halves of the constraint are visible on a published
  release, not inferred from plugin source.
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
