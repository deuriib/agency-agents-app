# 261104_updater-package-installs

## Objective

Three defects in one surface — the Updates card and the network posture behind it — all
reported against `v0.8.3`, and all a place where the app claimed something it could not do.

- **#56** — a `.deb` / `.rpm` install failed *every* update with `invalid updater binary
  format`, after downloading the full ~85 MB, and reported it as a **network** error.
  `Try again` re-downloaded the artifact and failed identically, forever.
- **#57** — the card rendered the release body as text while that body is generated
  Markdown, so the user read `## [0.8.3](https://…)`, list markers and bare URLs.
- **#47** — three places still advertised `agencyagents.app`, a host the app stopped
  contacting when the check moved to GitHub Releases. One of them was the CSP.

They were fixed together because all three touch `types.ts` and the locale files;
separate PRs would have guaranteed conflicts.

## Root cause, traced

**#56 is structural, not a bug to patch.** Two halves disagree and neither can be fixed
alone:

1. `tauri-utils` writes a bundle-type sentinel into the binary when the bundler produces a
   `.deb` / `.rpm` (`tauri-utils-2.9.2/src/platform.rs:349-370`), so the running app
   reports `BundleType::Deb`.
2. `tauri-plugin-updater 2.10.1` dispatches on it —
   `installer_for_bundle_type(bundle_type())` (`updater.rs:969`, `:1431`) → `Installer::Deb`
   → `install_deb`, which validates the payload and rejects anything that is not a real
   package (`:1049-1056`, error at `error.rs:80`).
3. A manifest can only carry **one** artifact per OS/arch:
   `tauri_plugin_updater::target()` builds the key as `{os}-{arch}` (`:1315-1321`) with no
   installer dimension. Our `linux-x86_64` entry is the AppImage tarball.
4. So the package install hands an AppImage to `install_deb`, deterministically, every
   time. Pointing the manifest at the `.deb` would fix package installs and break the
   AppImage — giving up the artifact that works today to rescue the one that does not.

Our own code had no bundle-type gate at all: `run_install` gated on `cached.installable`,
which is a property of the *release* ("does it carry `latest.json`?"), never of the
machine. And `translate_plugin_error` funnelled everything that was not a signature
failure into `AppError::Network` — the copy that sends a user to check a connection that
is working, while hiding the one actionable sentence.

**#57 was a field that changed under its renderer.** `release-please` writes the changelog
as Markdown; `convert_raw_release` copied `body` into `notes` verbatim; the card
interpolates `{info.notes}` as text with no Markdown pass and no dependency that could
render it. Two comments asserted the opposite of the code (`updater.rs:147`,
`types.ts:217-219`), which is why it read as intentional: "render as-is" was correct when
`notes` was one short sentence.

**#47 was drift after a migration.** The updater endpoint moved to
`releases/latest/download/latest.json` long ago; the CSP, the network panel row and two
comments did not move with it.

## Outcome

- ✅ **A pure bundle-type gate.** `blocked_package_format(Option<BundleType>)` →
  `Some(Deb | Rpm)` or `None`. AppImage, macOS `.app` / `.dmg`, Windows `.msi` / NSIS and
  `None` (a `tauri dev` binary, which carries no sentinel) are unchanged, byte for byte.
  No Tauri runtime, no network, no built artifact needed to test it.
- ✅ **The capability is in the payload, not guessed by the UI.**
  `UpdateCheckOutcome::Available` gained `inAppInstall` (a property of the *build*) and
  `packageDownload` (the release's matching installer). Deliberately not folded into
  `installable`: a release can carry a perfectly good manifest that a given build still
  cannot consume, and collapsing the two is what produced the bug.
- ✅ **Refused before the download.** `run_install` gates through `self_update_gate()`
  ahead of `backend.download_and_install`, so the ~85 MB is never fetched.
- ✅ **The scheduler stops churning.** `auto_install_blocker` gained
  `AutoInstallSkip::UnsupportedBundle` via an internal
  `auto_install_blocker_for_bundle(state, version, bundle)`; the public signature is
  unchanged. Without it the scheduler emits `Started` then `Failed` on every wake — a
  journal row per day for an outcome no release can change.
- ✅ **The error is classified honestly.** `translate_plugin_error` was
  `#[cfg(not(test))]` with zero coverage; the decision is now a pure
  `classify_plugin_error(msg, context)`. An installer-format rejection maps to
  `InvalidUpdaterFormat`, the pre-flight gate to `SelfUpdateUnsupported`. Neither is ever
  `Network`.
- ✅ **The card tells the truth.** `Download .deb` / `.rpm` (deep-linking the release's own
  asset) plus one sentence saying why; the auto-install toggle is disabled once a check
  proves it cannot work, with the same sentence as its hint.
- ✅ **Release notes are flattened at the source.** `strip_markdown` handles headings,
  bullets, thematic breaks, inline links (targets with their own parentheses, emphasis
  nested in the label), `**bold**` / `__bold__` / `*italic*`, and code spans. Hand-written
  scanner: no dependency, no HTML, so `{info.notes}` stays auto-escaped and no sanitizer
  is needed.
- ✅ **Flat, then bounded.** `bounded_notes` flattens *before* `MAX_NOTES_CHARS` applies.
  Truncating first could cut a link or URL in half; this order makes that class of bug
  unrepresentable instead of something to guard against.
- ✅ **The retired host is gone.** Removed from the CSP `connect-src`, from Settings →
  Network (the three GitHub rows already name every origin the check and the download
  use), and from `network.updaterDesc` in all 12 locales. Two comments reworded to "release
  host" while keeping the minisign threat rationale, which still holds.

## Measured

| | before | after |
|---|---|---|
| Rust tests (`cargo test --lib`) | 596 | **626** (+30) |
| Frontend tests (Vitest) | 90 | 90 |
| i18n | 104 guarded keys × 11 locales | 104 (3 `updates.*` added, 1 `network.*` removed) |
| New dependencies | — | **0** |
| Files | — | 23 (+1304 / −52) |

CI: **10/10 checks green** on PR #102 before merge. `mise run ci` locally in 305s.

## Not verified (honest status)

- **No visual evidence.** The app was not launched, so #47's "renders correctly in a long
  locale (`de`, `ru`) with no overflow" is **unconfirmed**. The change only removes a row
  from that panel, so the overflow risk decreases — but that is reasoning, not a
  screenshot.
- **No frontend unit test** for the card's new branch or for the store lifting
  `inAppInstall` / `packageDownload`. Covered by `svelte-check` and the production build.
- **The Deb/Rpm path is not exercised through `run_install`**, because the gate reads the
  host's bundle type. The *decision* is covered exhaustively by
  `blocked_package_format_covers_every_bundle_type` (8 inputs) and
  `self_update_gate_reports_the_package_format`; the wiring between them is a single `?`.
- **36 translations written by the author.** `en` / `es` / `pt-BR` are native;
  `de`, `fr`, `ru`, `tr`, `ja`, `ko`, `zh-CN`, `zh-TW`, `fa` are best-effort and
  unreviewed — the same standing caveat as the rest of the locales.
- **Not fixed, deliberately:** #56 option B (per-installer manifests +
  `latest-deb.json` / `latest-rpm.json`). It needs a signing step for `.deb` / `.rpm`,
  which carry no `.sig` today. Documented in `docs/updater-release.md`.

## Files

**Modified**

- `src-tauri/src/github/releases.rs` — `PackageFormat`, `PackageAsset`,
  `ReleaseInfo::package_assets`, `strip_markdown` + helpers, `bounded_notes`,
  `is_github_https` shared with `safe_release_url`
- `src-tauri/src/commands/updater.rs` — `blocked_package_format`, `package_download_for`,
  `self_update_gate`, `local_bundle_type`, `classify_plugin_error`, the capability fields,
  both gates
- `src-tauri/src/error.rs` — `SelfUpdateUnsupported`, `InvalidUpdaterFormat`
- `src-tauri/src/github/mod.rs` — re-export the new types
- `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json` — retired host
- `src/lib/types.ts` — `PackageFormat`, `PackageAsset`, the two payload fields, two
  `AppErrorPayload` cases, two `appErrorMessage` cases
- `src/lib/stores/updater.svelte.ts` — lift the new fields
- `src/lib/components/SettingsSectionUpdates.svelte` — `!inAppInstall` affordance, the
  disabled auto-install toggle, `.package-blocked`
- `src/lib/components/SettingsSectionNetwork.svelte` — the retired row deleted
- `src/lib/i18n/locales/*.ts` (12) — +3 `updates.*`, −1 `network.updaterDesc`
- `docs/updater-release.md` — "Linux package installs are download-only" + two
  troubleshooting rows

## Follow-ups

- **#56 option B**, if Linux package installs should ever self-update: emit per-installer
  manifests and sign the `.deb` / `.rpm` artifacts. The app already re-resolves its
  endpoint per install (`self.updater(manifest_url)`), so the plumbing exists; the signing
  step does not.
- **Screenshots** of Settings → Network in `de` / `ru` to close #47's last criterion.
- **A Vitest case** for the store's `checkNow` lift, which would also be the first test of
  `updater.svelte.ts`.
