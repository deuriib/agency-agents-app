# 261103_cargo-lock-sync

## Objective

Issue #81. `release-please` bumps `src-tauri/Cargo.toml` but cannot bump
`src-tauri/Cargo.lock`, so the lock lagged one release behind: every
contributor's first `cargo build` dirtied the tree with an unrelated one-line
change, and no gate noticed.

## Outcome

- ✅ **Gate**: `cargo metadata --locked` as the `cargo-lock` job in `pr-check.yml`
  and as `mise run check-lock`. It fails exactly when the lockfile would have to
  change, needs no compile, and reads the manifest only.
- ✅ **Automation**: `release-lock.yml` runs on the `release-please--**` branch,
  refreshes the lock and commits it **into the release PR** — so the tagged commit
  carries a lockfile that matches its manifest, unlike a post-release commit.
- ✅ **Immediate drift fixed**: the entry was at `0.9.0` with the crate at `0.9.3`;
  `mise run sync-lock` moved it, and the diff is that one line.
- ✅ Documentation corrected: `release-please.yml` claimed the lock "is refreshed
  by the next cargo command (the CI build does it)", which never reaches the
  repository. `CONTRIBUTING.md` documents the gate and the one-command fix.

## Why the lock is not an `extra-files` entry

`release-please`'s `extra-files` updaters address a **jsonpath**. The lock's
target is an entry inside an array of tables
(`[[package]] name = "agency-agents-app"`), which that updater cannot express — a
different mechanism is required, not another config line.

## Why the release PR, not a post-release commit

A post-release commit leaves the tag itself with the stale lock, so anyone
building the tag still gets the dirty tree. Committing on the release branch puts
the sync inside the commit that gets tagged.

The known trade-off: a push made with `GITHUB_TOKEN` does not trigger new workflow
runs, so the release PR's checks do not re-run on the sync commit. That is
acceptable because those checks require a maintainer to start them anyway (noted
in `release-please.yml`), which happens after this job lands.

## Verified

- With the pre-fix lock (`0.9.0`) `mise run check-lock` fails with
  `cannot update the lock file ... because --locked was passed`.
- With the fix (`0.9.3`) it exits 0, and a second `mise run sync-lock` is a no-op.

## Files

**New**

- `.github/workflows/release-lock.yml`

**Modified**

- `.github/workflows/pr-check.yml` — `cargo-lock` job
- `.github/workflows/release-please.yml` — corrected the comment
- `.mise.toml` — `check-lock`, `sync-lock`; `ci` includes the gate
- `CONTRIBUTING.md`
- `src-tauri/Cargo.lock` — the one-line sync

## Follow-ups

- `scripts/bump-version.mjs` already rewrites the same entry for the local
  `release.sh` path; the two mechanisms now agree on the outcome, and the gate
  proves it on every PR.
