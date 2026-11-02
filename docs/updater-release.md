# Releasing updates

Agency Agents updates itself through `tauri-plugin-updater`. The contract
between a release and every installed app is a single file — `latest.json` —
attached to the GitHub release:

```json
{
  "version": "0.8.0",
  "notes": "…",
  "pub_date": "2026-11-02T12:00:00Z",
  "platforms": {
    "darwin-aarch64": { "signature": "…", "url": "…" },
    "windows-x86_64": { "signature": "…", "url": "…" }
  }
}
```

If that file is missing the endpoint answers 404, the updater swallows the
error, and every client reports "up to date" forever. That is the failure this
pipeline exists to prevent, so a release is not done until `latest.json` is on
it *and* reachable by an anonymous client.

This page is the runbook. `docs/BUILD.md` still describes the retired
`agencyagents.app` endpoint; that host is no longer used for updates.

## Channels

| Channel | Manifest URL | Selected release |
| --- | --- | --- |
| stable | `https://github.com/deuriib/agency-agents-app/releases/latest/download/latest.json` | `GET /repos/deuriib/agency-agents-app/releases/latest` — the newest published, non-prerelease release |
| beta | `https://github.com/deuriib/agency-agents-app/releases/download/updater-beta/latest.json` | first published prerelease in `/releases?per_page=30`, falling back to the stable release when there is none |

The platform keys are Tauri's and are not negotiable:

```text
darwin-aarch64  darwin-x86_64  windows-x86_64  windows-aarch64  linux-x86_64
```

A key that is spelled differently is silently ignored by the updater, which is
why `scripts/updater-manifest.mjs` refuses to emit anything else.

The app picks the channel from Settings → Updates. The beta channel is a fixed
URL, so switching channels never requires a new release; it changes which
manifest the next check reads. When the beta channel falls back to the stable
release, the app uses the stable URL — a beta client must never be handed a
manifest that the beta release does not own.

## Required secrets

### GitHub Actions (repository secrets)

Set once in **Settings → Secrets and variables → Actions**:

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | contents of the minisign private key (`~/.config/agency-agents-app/updater.key`) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password of that key (empty file/string if it has none) |

Without reading the key into your shell history or a log:

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.config/agency-agents-app/updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD   # prompts, does not echo
gh secret list
```

Or run the automated path from the repo on the machine that holds the key:

```sh
mise run set-updater-secrets
```

It reads the Keychain (or `--key-file`), compares the private key id with the
embedded `pubkey`, refuses to upload on a mismatch, and pipes both values to
`gh secret set` through stdin — never argv, a log, or shell history.

The public half is already embedded in `src-tauri/tauri.conf.json` →
`plugins.updater.pubkey`. Rotating the key means regenerating both halves **and**
shipping a release signed with the old key first, or installed apps will reject
every future update.

`GITHUB_TOKEN` needs no setup; the workflows request `contents: write`.

### Local macOS builds

`scripts/release.sh` reads the same key from the Keychain instead of the
environment (`agency-agents-updater-key` and `…-key-pw`; setup is documented at
the top of the script). It also requires an authenticated `gh`
(`gh auth login`, `repo` scope) and fails immediately if either is missing.

### No macOS? Generate the key on Linux/Windows

The signing key is a minisign keypair; it does not require macOS. Generate it on
the machine that will hold it:

```sh
mkdir -p ~/.config/agency-agents-app
npx tauri signer generate -w ~/.config/agency-agents-app/updater.key
chmod 600 ~/.config/agency-agents-app/updater.key
```

`updater.key.pub` contains the one-line value for
`plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. Update that value on a
branch, commit it, and only then run:

```sh
mise run set-updater-secrets
```

The script compares the private key id with the `pubkey` in the working tree and
refuses to upload a mismatch. On Linux/Windows it reads the file and prompts for
the password; no Keychain is required.

**Rotating the key is a one-way door for installed clients.** They embed the old
`pubkey` and reject updates signed with the new one. If you still control the old
private key, ship a transition release signed with it first. If it is lost — for
example, a fork whose upstream key lived in a Mac Keychain — users must install
one release manually; every release after that updates normally.

## What each build produces

| Platform | Uploaded to the release | Signed by |
| --- | --- | --- |
| Linux (CI) | `<product>_<version>_amd64.AppImage`, `…AppImage.tar.gz`, `…AppImage.tar.gz.sig`, `.deb`, `.rpm` | CI job `linux` |
| Windows (CI) | `<product>_<version>_x64-setup.exe` + `.sig`, `…_arm64-setup.exe` + `.sig` | CI job `windows` |
| macOS (local) | `Agency Agents_<version>_<arch>.dmg`, `Agency.Agents_<version>_<aarch64\|x64>.app.tar.gz` + `.sig` | `scripts/release.sh` |
| Manifest (CI or local) | `latest.json` | n/a |

Linux is the one platform where the updater payload is **not** the tarball Tauri
produced: the AppImage is repacked after bundling to un-bundle host Wayland
libraries (#57), so `linux-build.yml` deletes Tauri's tarball, rebuilds it from
the repacked AppImage, and signs it again. A `linux-x86_64` entry whose
signature was made over the pre-repack file fails verification with
`signature verification failed` — that is a broken build, not a corrupt
download.

## Release flow

1. **Merge the release PR.** release-please turns Conventional Commits on `main`
   into a version bump, a `vX.Y.Z` tag, and a **draft** release with the
   changelog as its body.
2. **CI builds Linux, then Windows, then the manifest.** `release-please.yml`
   chains `linux` → `windows` → `manifest`. The manifest job downloads the
   assets the two platform jobs attached, builds `latest.json` from them
   (`--merge` keeps any platforms already present, so re-runs are safe), and
   attaches it to the same draft. It never publishes.
3. **Build macOS locally.** On the Mac with the Developer ID and the Keychain
   entries:

   ```sh
   ./scripts/release.sh
   ```

   It builds and notarizes both arches, uploads the DMGs plus the
   `Agency.Agents_<version>_<arch>.app.tar.gz` payloads and signatures,
   downloads the release's `latest.json`, merges the `darwin-*` entries into it,
   uploads it back, and then publishes the release.
4. **Verifying before you publish.** Set `NO_PUBLISH=1` to stop after the upload:

   ```sh
   NO_PUBLISH=1 ./scripts/release.sh
   gh release view "v0.8.0" --json isDraft,assets --jq '{isDraft, assets: [.assets[].name]}'
   ```

   Publish later with `gh release edit v0.8.0 --draft=false`.
5. **Check the result.** See [Verifying a manifest](#verifying-a-manifest).

Nothing is published until every asset a client needs is attached: the
`publish` job runs only after `linux`, `windows` and `manifest` succeed, and it
is skipped when any of them fails.

### CI-only releases (no macOS)

Forks without a Mac publish Linux and Windows from CI. The `publish` job in
`release-please.yml` flips the draft after `linux`, `windows` and `manifest`
succeed. It is on by default; set the repository variable `AUTO_PUBLISH=false`
to restore the draft-then-human-publish flow:

```sh
gh variable set AUTO_PUBLISH -R deuriib/agency-agents-app --body false
```

The resulting `latest.json` contains only the platforms CI built. Verify it with
the explicit platform list:

```sh
mise run verify-updater -- \
  --expect linux-x86_64,windows-x86_64,windows-aarch64 \
  --check-urls
```

macOS is not part of a CI-only release: there is no `darwin-*` entry and a macOS
client cannot install it. Add a `macos-latest` job (and a signing identity) if
that platform matters.

### Beta releases

A version whose semver carries a prerelease suffix (`0.8.0-beta.1`) is handled
by the same flow, with two additions:

- The release is marked prerelease, so it is excluded from
  `releases/latest` and the stable channel never sees it.
- `latest.json` is also copied to the long-lived `updater-beta` release
  (`--clobber`, so it always holds the newest beta). CI stages that copy as soon
  as the platform builds finish; `scripts/release.sh` mirrors it right after
  publishing. The mirror only becomes reachable to clients once the prerelease
  itself is published — draft assets 404 for anonymous clients.

`updater-beta` must never be the repository's "latest" release, or the stable
endpoint would start handing betas to everyone. Both `gh release create` calls
pass `--latest=false` for that reason; do not flip it by hand.

## Verifying a manifest

The repo ships the same checks as a task:

```sh
mise run verify-updater                       # stable channel, all five platforms
mise run verify-updater -- --channel beta     # beta channel
mise run verify-updater -- --check-urls       # also HEAD every artifact URL
mise run verify-updater -- --file dist/latest.json --allow-subset
```

It exits non-zero on a missing platform, an unknown platform key, an empty
signature, a non-https URL, or (with `--check-urls`) an artifact that is not
downloadable. `--json` emits a machine-readable report.

After publishing, exactly what a client sees:

```sh
TAG=v0.8.0
curl -fsSL "https://github.com/deuriib/agency-agents-app/releases/latest/download/latest.json" | jq .
curl -fsSL "https://github.com/deuriib/agency-agents-app/releases/download/${TAG}/latest.json" | jq .
```

Expected: every platform you shipped, each with a non-empty `signature` and an
`https` `url` ending in the artifact's filename. Then confirm the artifacts are
actually downloadable (a manifest is worthless if it 404s):

```sh
curl -fsSIL "$(curl -fsSL "https://github.com/deuriib/agency-agents-app/releases/download/${TAG}/latest.json" \
  | jq -r '.platforms["linux-x86_64"].url')" | head -1
```

Before publishing, inspect the draft instead:

```sh
gh release download "$TAG" --pattern latest.json --dir /tmp/agent-manifest --clobber
jq -r '.version, (.platforms | keys[])' /tmp/agent-manifest/latest.json
```

`scripts/updater-manifest.mjs` is the only supported way to produce the file: it
validates the semver, the https base URL, the platform keys, the artifact paths
and each `.sig`, and writes nothing at all if one check fails. Building it by
hand is how a release ends up with a signature for the wrong file.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| App always says "up to date" | `latest.json` missing, or the release is still a draft (404 to anonymous clients) | Check `gh release view <tag>`; publish the release |
| Manifest lacks a platform | That platform's job failed, or its `.sig` was not attached | Re-run the job; the manifest job rebuilds from whatever is on the release |
| `signature verification failed` after download | Artifact and `.sig` come from different builds (typically the Linux pre-repack tarball) | Re-run the Linux build; both files are regenerated together |
| Beta channel serves stable | No published prerelease, or `updater-beta` is missing | Check `gh release view updater-beta`; the app falls back to stable by design |
| Stable channel offers a beta | A prerelease was marked as "latest", or published without the prerelease flag | `gh release edit <tag> --prerelease` and re-check `/releases/latest` |
| `latest.json` points at a renamed asset | An asset was replaced without regenerating the manifest | Re-run the manifest step; it is derived from filenames, never from a template |
| CI fails with `TAURI_SIGNING_PRIVATE_KEY is empty` | The repository secrets were never set | `mise run set-updater-secrets` on the machine holding the key; Linux/Windows is fine |
| An installed app rejects the new release after a key rotation | The installed binary embeds the old `pubkey` | Ship a transition release signed with the old key, or install one release manually |
| macOS offers an update that fails | A CI-only manifest has no `darwin-*` entry | Add a `macos-latest` build job, or ignore macOS in this fork |
