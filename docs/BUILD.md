# Building Agency Agents

## Prerequisites: the pinned submodules

Two upstream repositories are pinned as git submodules under `vendor/`
(`agency-agents` and `agency-rules`), pinned to release tags:

```sh
git submodule update --init --recursive   # or clone with --recurse-submodules
```

Everything the app embeds is derived from those checkouts by
`scripts/sync-vendor.mjs`:

| Generated artifact | Source |
| --- | --- |
| `src-tauri/data/tools.json` | `vendor/agency-agents/tools.json` + `src-tauri/data/tools-overlay.json` |
| `src-tauri/data/rules-tools.json` | `vendor/agency-rules/tools.json` |
| `src-tauri/resources/corpus-baseline/` | `vendor/agency-agents` divisions + `divisions.json` + `strategy/` + `scripts/` |
| `src-tauri/resources/rules-baseline/` | `vendor/agency-rules/rules/` |

```sh
mise run sync-vendor    # regenerate after bumping a submodule
mise run check-vendor   # what CI runs: fails when anything is stale
```

`src-tauri/build.rs` checks for both submodules before anything compiles. A
clone without them stops with:

```text
vendor/agency-agents is empty.

The app embeds the catalog and rule contracts from pinned git
submodules. Initialise them and rebuild:

    git submodule update --init --recursive
```

A GitHub source archive (`/archive/…tar.gz`, "Download ZIP") contains no
submodule content at all, so it cannot be built; clone the repository instead.

The only sanctioned divergence from upstream is
`src-tauri/data/tools-overlay.json`: one named field per entry with its reason,
rejected when it stops differing from upstream. Everything else in
`tools.json` must match `vendor/`.

## Development Build

```sh
npm install
npm run tauri dev
```

The dev server runs on port `1430`. The app opens with HMR for frontend changes; Rust changes trigger a backend rebuild.

## Local QA Batch

The repeatable Phase C batch is:

```sh
npm run build:phase-c
```

It runs the local build/test checks and static platform-config validation.

When the configured Ubuntu and Windows VM targets are available:

```sh
npm run build:phase-c:full
```

This includes VM-assisted packaging checks. Do not claim native runtime verification for an OS unless the app was actually launched there.

## Standard Checks

```sh
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run check
npm run build
```

Renderer parity. Both parity tests default to the pinned submodules, so they run
on a clean clone with no environment variables; `AGENCY_*_PARITY_ROOT` only
overrides the checkout:

```sh
mise run parity                     # defaults to vendor/agency-agents
```

`vendor/agency-agents` no longer ships `scripts/convert.sh` — the catalog moved
to `python -m agency_agents` and publishes `convert-outputs.sha256` as its byte
oracle. The bash-based parity test therefore reports "skipping" against the
pinned tag; point `AGENCY_AGENTS_PARITY_ROOT` at an older checkout that still
has the script, or wait for the manifest-based oracle (issue #76).

Rules parity against `vendor/agency-rules`:

```sh
AGENCY_RULES_PARITY_ROOT=$PWD/vendor/agency-rules \
cargo test --manifest-path src-tauri/Cargo.toml --test rules_conformance \
  rules_parity_against_checkout -- --ignored
```

The rules renderers are also checked byte-for-byte against the committed golden
fixtures on every `cargo test` (`tests/rules_conformance.rs`, 8 rules × 4
harnesses). The fixtures are generated output, not hand-written: see
`src-tauri/tests/fixtures/rules/PROVENANCE.md` for the reference commit and the
exact regeneration command. The `--ignored` parity run re-renders the live
checkout's `rules/` and diffs its `integrations/`; a rule that exists upstream
with no compiled artifact is reported as a finding, not a failure (that tree is
gitignored upstream and known to be stale — ruling C5).

**Expect failures when the checkout's `integrations/` is stale.** The bundle is
generated output, so an upstream rule edit that was never recompiled makes the
parity run report divergences for a *correct* renderer. Regenerate before
trusting a red run:

```sh
cd agency-rules && INTEGRATIONS_DIR=/tmp/rules-oracle node bin/agency-rules.mjs convert
```

Then diff the oracle against `integrations/` to separate "upstream never
recopied it" from "our renderer is wrong". Only the second is an app bug.

## Release Build On macOS

The release build produces a signed `.app` and `.dmg`.

### Prerequisites

`scripts/release.sh` reads **every secret from the macOS Keychain** — nothing lives in the repo
or an env file. The non-secret `APPLE_ID` / `APPLE_TEAM_ID` are hardcoded in the script. One-time setup:

1. Apple Developer ID Application certificate in the login keychain:

   ```sh
   security find-identity -v -p codesigning
   ```

2. Apple notarization password — an app-specific password from <https://appleid.apple.com>
   (Sign-In and Security → App-Specific Passwords), stored in the Keychain:

   ```sh
   security add-generic-password -a "<apple-id>" -s "agency-agents-notary" -U -w
   # paste the app-specific password when prompted
   ```

   Save the same password as a `notarytool` profile too. Tauri notarizes the `.app` but only *signs*
   the `.dmg` around it; `release.sh` uses this profile to notarize and staple each `.dmg` itself
   (Apple's guidance: notarize the outermost container). It prompts, so the password stays off the
   command line:

   ```sh
   xcrun notarytool store-credentials agency-agents --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID"
   ```

   `release.sh` reads `APPLE_ID` and `APPLE_TEAM_ID` from the environment (it does not hardcode an
   identity). Export them before running it, and use the same Apple ID for the Keychain entry above:

   ```sh
   export APPLE_ID="you@example.com"
   export APPLE_TEAM_ID="ABCDE12345"   # Developer ID Application: Your Name (ABCDE12345)
   ```

   `release.sh` checks the profile before building, which also surfaces an expired Apple developer
   agreement (HTTP 403 "A required agreement is missing or has expired" — the Account Holder accepts it
   at <https://developer.apple.com/account>) before a long build rather than after.

3. Updater signing key — the minisign key whose public half is embedded in `tauri.conf.json`.
   Generate it once (writes the canonical key file), then store the key **and** its password in
   the Keychain:

   ```sh
   npm run tauri -- signer generate -w ~/.config/agency-agents-app/updater.key
   # paste the PUBLIC key it prints into tauri.conf.json → plugins.updater.pubkey

   security add-generic-password -a agency-agents -s agency-agents-updater-key    -U -w "$(cat ~/.config/agency-agents-app/updater.key)"
   security add-generic-password -a agency-agents -s agency-agents-updater-key-pw -U -w
   # (paste the key's password for the second one)
   ```

   Store the key via `$(cat …)`, not a manual paste — a stray trailing newline corrupts it and
   signing fails with `incorrect updater private key password: Invalid input`. (Repair script:
   re-run the `agency-agents-updater-key` line above from the canonical file.)

### Build

```sh
./scripts/release.sh
```

That's it — `release.sh` pulls the notary + updater secrets from the Keychain, then builds, signs,
notarizes, staples, and (unless `SKIP_UPDATER=1`) emits the signed updater artifacts. It builds both
Mac arches by default; override with e.g. `RELEASE_TARGETS="aarch64-apple-darwin"`.

> **Intel cross-compile** needs the **rustup** toolchain — Homebrew's `rust` is host-only and yields
> `can't find crate for core` for `x86_64-apple-darwin`. Add the target once
> (`rustup target add x86_64-apple-darwin`) and run with
> `PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH" RELEASE_TARGETS="x86_64-apple-darwin" ./scripts/release.sh`.

If using the lower-level Tauri build directly:

```sh
npm run tauri build
```

Expected artifacts live under:

```text
src-tauri/target/release/bundle/
```

## Verify macOS Artifacts

```sh
DMG=src-tauri/target/release/bundle/dmg/Agency\ Agents_0.1.0_aarch64.dmg

codesign -dv --verbose=4 "$DMG"
spctl --assess --type install --verbose=4 "$DMG"   # expect: source=Notarized Developer ID
xcrun stapler validate "$DMG"                      # expect: The validate action worked!
```

The exact filename may vary by version and architecture.

## Updater Manifest

Agency Agents uses `tauri-plugin-updater`. The signed manifest is
`latest.json`, attached to each published GitHub release. The configured
stable endpoint is:

```text
https://github.com/deuriib/agency-agents-app/releases/latest/download/latest.json
```

The updater artifact is a platform bundle (`.app.tar.gz`, NSIS `.exe`, or
AppImage `.tar.gz`), not the `.dmg`.

`latest.json` is generated by `scripts/updater-manifest.mjs`, which merges the
platforms produced by CI (Linux, Windows) and by the local macOS build. The
updater public key is embedded in `src-tauri/tauri.conf.json`; the matching
private key is read from GitHub Actions secrets in CI and from the **Keychain**
on a local macOS build (`agency-agents-updater-key` + `…-key-pw`).

The full runbook — required secrets, per-platform artifacts, channel behavior,
publish/verify steps, and troubleshooting — is in
[docs/updater-release.md](./updater-release.md). This section intentionally
does not duplicate it.

## Release Checklist

The ordered runbook for cutting a release. The mechanics referenced here are detailed in the sections above.

### Decisions for v0.1.0 (first release)

- **Version:** `0.1.0` — already set in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`. No bump needed.
- **Distribution:** signed + notarized `.dmg`, manual download.
- **Auto-update: deferred (historical).** The updater public key shipped, but the endpoint was not provisioned in v0.1.0. This was superseded by the GitHub-release `latest.json` pipeline — see [docs/updater-release.md](./updater-release.md).
- **Out of scope (known limitations, noted in the release notes):** auto-update, multi-file renderers (antigravity / openclaw / aider / windsurf), Windows/Linux runtime verification, and the local-runtime (Ollama / LM Studio) target.

### Steps

1. **Pre-flight — confirm green on a clean `main`:**

   ```sh
   (cd src-tauri && cargo test --lib)                 # expect 258/0
   AGENCY_AGENTS_PARITY_ROOT=<clone> cargo test --manifest-path src-tauri/Cargo.toml --lib \
     upstream_convert_sh_is_byte_identical_for_transform_tools -- --ignored   # expect 1160/1160
   npm run check                                      # 0 errors
   npm run build                                      # clean
   ```

   Or the bundled batch: `npm run build:phase-c`.

2. **Finalize release notes** — `git mv docs/release-notes/unreleased.md docs/release-notes/0.1.0.md`, retitle to `Agency Agents v0.1.0 - <date>`, drop the staging-file line, then recreate an empty `unreleased.md` stub for the next cycle.

3. **Build** — `SKIP_UPDATER=1 ./scripts/release.sh` (see *Release Build On macOS*). Produces the signed, notarized, stapled `.app` + signed `.dmg`.

4. **Verify artifacts** — run the `codesign` / `spctl` / `stapler` checks (see *Verify macOS Artifacts*).

5. **Smoke test** — mount the `.dmg`, launch, confirm Gatekeeper acceptance, the first-run catalog picker, and the Tahoe glass icon.

6. **Publish** — `git tag v0.1.0 && git push origin v0.1.0`, then `gh release create v0.1.0 <dmg> --notes-file docs/release-notes/0.1.0.md`.

7. **Post-release** — log the cut in `memory-bank/agentLog.md`; open the next milestone for the deferred items (updater endpoint, Phase 5 quality gate).

### Auto-update publishing

Auto-update is published from the GitHub release itself. `scripts/release.sh`
builds and notarizes macOS, uploads the DMGs plus the signed `.app.tar.gz`
payloads, merges the `darwin-*` entries into the release's `latest.json`, and
publishes the release. Linux and Windows payloads plus the manifest are attached
by CI. See [docs/updater-release.md](./updater-release.md) for the ordered
runbook, the beta channel, and the verification commands.

The old `agencyagents.app/updater.json` endpoint and
`tools/release/publish-manifest.sh` are retired: the former served a stale
v0.3.2 manifest pointing at the upstream owner, which is exactly the failure
this pipeline replaces.

## macOS Icon Notes

macOS 26 Tahoe uses the compiled `Assets.car` path for Liquid Glass icons. Do not blindly run `npm run tauri icon`; it can clobber the curated icon outputs.

See [docs/icon/README-liquid-glass.md](./icon/README-liquid-glass.md).

## Cross-Platform Notes

- macOS uses overlay titlebar, vibrancy, and the Tahoe icon setup.
- Windows and Linux use native decorated opaque windows.
- Windows Intel and ARM builds should be verified as separate artifacts.
- Linux packages should be smoke-tested in the Ubuntu VM before claiming support.

## Troubleshooting: proc-macro "can't find crate" on a beta macOS

On a **beta macOS / Xcode** (first hit on macOS 26/27 Tahoe beta during the v0.1.0 cut), `scripts/release.sh` can fail with:

```
error[E0463]: can't find crate for `ctor_proc_macro`   (or tauri_macros / serde_with_macros / …)
```

**Root cause:** the Tauri CLI sets `MACOSX_DEPLOYMENT_TARGET` (from `bundle.macOS.minimumSystemVersion`, `13.0`) before invoking cargo, and on the beta toolchain that breaks *fresh* proc-macro dylib compilation. The exact same `cargo build --bins --features tauri/custom-protocol --release` succeeds **bare** (no `MACOSX_DEPLOYMENT_TARGET`) and on the **CI runners** (stable macOS) — so prefer CI for releases when possible. It is non-deterministic (a different proc-macro fails each run) and is NOT a code problem.

**Local recovery** — pre-compile the crate graph (incl. proc-macros) *without* `MACOSX_DEPLOYMENT_TARGET`, using Tauri's exact config so it won't recompile, then let `release.sh` bundle/sign/notarize off the warm cache (proc-macro fingerprints don't include `MACOSX_DEPLOYMENT_TARGET`, so they're reused):

```sh
# 1. Capture the exact TAURI_CONFIG the CLI passes to cargo (one-shot; the run
#    is expected to fail at a proc-macro — we only need the dumped value).
cat > /tmp/dump.sh <<'W'
#!/bin/bash
[ -f /tmp/tc.txt ] || printf '%s' "$TAURI_CONFIG" > /tmp/tc.txt
exec "$@"
W
chmod +x /tmp/dump.sh; rm -f /tmp/tc.txt
RUSTC_WRAPPER=/tmp/dump.sh npm run tauri build -- --config '{"bundle":{"createUpdaterArtifacts":false}}' >/dev/null 2>&1 || true
TC=$(cat /tmp/tc.txt)

# 2. Per arch: pre-build WITHOUT MACOSX_DEPLOYMENT_TARGET, then bundle.
#    (For the non-host arch, use a rustup toolchain that has the target's std.)
rm -rf src-tauri/target
env -u MACOSX_DEPLOYMENT_TARGET TAURI_CONFIG="$TC" \
  cargo build --bins --features tauri/custom-protocol --release   # host arch
SKIP_UPDATER=1 RELEASE_TARGETS="aarch64-apple-darwin" ./scripts/release.sh

env -u MACOSX_DEPLOYMENT_TARGET TAURI_CONFIG="$TC" \
  cargo build --bins --features tauri/custom-protocol --release --target x86_64-apple-darwin
SKIP_UPDATER=1 RELEASE_TARGETS="x86_64-apple-darwin" ./scripts/release.sh
```

The cleaner long-term fix is to cut macOS releases on a stable macOS box or a macOS CI runner. (Homebrew/CLT vs rustup also matters for the x86_64 cross — that target's std lives in the rustup toolchain; put `~/.rustup/toolchains/stable-<host>/bin` first on `PATH`.)

## Secrets

Never commit signing credentials, updater private keys, GitHub tokens, or notary credentials. Use local keychain items or files outside the repo with `0600` permissions.

### Release automation token

`release-please` pushes the release branch and `release-lock.yml` pushes the
lockfile sync onto it. Both must push with a credential that is **not**
`GITHUB_TOKEN`: a `GITHUB_TOKEN` push starts no workflow run, so the lock sync
never fired and the release PR's checks never ran on it (issue #91).

| Name | Kind | Purpose |
| --- | --- | --- |
| `RELEASE_APP_CLIENT_ID` | repository **variable** (not a secret) | identifies the release App in `client-id:` |
| `RELEASE_APP_PRIVATE_KEY` | repository **secret** | the App's private key; mints the installation token |

One-time setup: create an App (suggested name `agency-agents-release`) under
Settings → Developer settings → GitHub Apps → New, with no webhook and no OAuth
callback, and install it on this repository only. Grant these installation
permissions:

| App permission | Why |
| --- | --- |
| Contents: read and write | push the `release-please--**` branch, create the `vX.Y.Z` tag and the draft release |
| Pull requests: read and write | open and update the release PR |
| Issues: read and write | apply the `autorelease: pending` / `autorelease: tagged` labels |

Each workflow narrows the minted token to what its own job does: `release-please.yml`
asks for exactly the three above, while `release-lock.yml` asks for `contents: write`
alone — it only commits one file and pushes it. A `permission-*` entry the
installation does not grant fails the token step outright, so a permission
mismatch is a loud failure rather than a degraded run.

Set once, values never echoed:

```sh
gh variable set RELEASE_APP_CLIENT_ID -R deuriib/agency-agents-app \
  --body "<app client id>"   # App settings → Client ID
gh secret set RELEASE_APP_PRIVATE_KEY -R deuriib/agency-agents-app \
  < ~/downloads/release-app.private-key.pem
gh secret list -R deuriib/agency-agents-app   # names only — never prints a value
```

The key goes in through a redirect; `gh secret set` never receives it on argv, so
nothing lands in shell history. Delete the downloaded `.pem` after the upload.

**Rotation**: App settings → *Private keys* → *Generate a private key*, then
`gh secret set RELEASE_APP_PRIVATE_KEY -R deuriib/agency-agents-app < <new>.pem`, then delete the old key in
the App settings. The secret name never changes, so no workflow edit is needed.

**Failure mode**: there is no fallback credential. If the key is missing,
misnamed, or revoked, the mint step fails and the job fails — which is the point,
because a green run on `GITHUB_TOKEN` silently restores the #91 behaviour. If the
key is valid but the App lost a permission, the same step fails with the API error
naming that permission.

The token itself is a short-lived installation token (one hour, revoked at the end
of the job); the private key is the only long-lived material.
