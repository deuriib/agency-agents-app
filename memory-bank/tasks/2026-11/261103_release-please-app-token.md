# 261103_release-please-app-token

## Objective

Issue #91. `release-please` pushed its `release-please--…` branch with the default
`GITHUB_TOKEN`, and GitHub suppresses workflow runs for pushes made with that
token. `release-lock.yml` therefore never fired once in its whole life, so
`src-tauri/Cargo.lock` stayed one release behind and the `cargo-lock` job failed
on `main` for two consecutive releases (`9084b5e` at 0.10.0, `3472831` at 0.11.0).

## Outcome

- ✅ **AC1 (wiring half)**: both workflows mint a GitHub App installation token
  (`actions/create-github-app-token`, pinned to `v3.3.0`) as their first,
  unconditional step, and pass it to the action that pushes — `token:` on
  `release-please-action`, `token:` on `actions/checkout` in `release-lock.yml`.
  There is deliberately no `GITHUB_TOKEN` fallback: a fallback keeps the job green
  while the sync stays dead, which is the bug being fixed.
- ✅ **AC6**: the false claims are gone — "No PAT or app token is needed" in
  `release-please.yml`, "the next cargo command refreshes the lock" in
  `CONTRIBUTING.md`, and "those checks require a maintainer to start them anyway"
  in `release-lock.yml`. The workflows now point at
  `docs/BUILD.md#release-automation-token`.
- ✅ **AC7**: `docs/BUILD.md` gains `### Release automation token` — both credential
  names, the App's exact three permissions and why each is needed, the
  redirect-based `gh` commands (nothing on argv, so nothing in shell history), and
  the rotation procedure.
- ✅ **Guard**: `scripts/release-workflows.test.mjs`, 13 test cases, runs in
  `npm run test:scripts` / the `scripts` CI job. Both workflows' mint steps are
  pinned — unconditional, `id:` consumed as `steps.<id>.outputs.token`, canonical
  `client-id`/`private-key` lines, and a set-equality scope (`contents`, plus
  `pull-requests`/`issues` for release-please only). On the pre-fix workflows 7 of
  the 13 are red, so the suite is a regression test and not a tautology.
- ⏳ **AC2–AC5 are post-merge and UNPROVEN.** No step of the mint → push → sync →
  repair path can execute in a working tree; they require the App to exist on the
  repository. They are not "achieved" by this change.

## Why the App token (not a different trigger)

- A `workflow_run` trigger on `Release Please` completion would run *after* the
  release branch exists but its commit would land outside the tagged commit — the
  precise defect `release-lock.yml` was written to avoid. It also inherits the same
  token without fixing why the event never happened.
- A `schedule` sweep hides latency and lets a stale lock reach a tag.
- Making `release-please` commit the lock itself is impossible: `extra-files` uses
  a jsonpath updater, and the lock's target is an entry inside an array of tables.
- A fine-grained PAT would work, but it is a person-scoped long-lived credential.
  An App installation token is scoped to the installation, narrowed per job by
  `permission-*`, valid for one hour and revoked in the action's post step. The App
  can also lose a permission without anyone rotating a personal token.

`permission-*` can only narrow what the installation grants, and asking for more
than the App has fails the token step — a permission mistake is a loud step
failure, never a degraded run.

## Why release-lock also pushes with it

The sync commit must itself be a real event. Pushed with `GITHUB_TOKEN`, it would
start no `pull_request` run, so `pr-check.yml`'s `cargo-lock` would keep describing
the head *before* the sync — the same bug one level down, and exactly the trade-off
this change supersedes.

Loop termination is deliberate and recorded because it looks like a failure: the
sync push triggers a second `Release lock sync` run in the same concurrency group,
which cancels the pusher (`conclusion: cancelled`) and then exits at
`git diff --quiet` without pushing. Steady state per release-PR update is one
`cancelled` pusher plus one `success` no-op. Verify AC2 against the **successful
no-op** and the presence of the sync commit, not against the pusher's conclusion.

## Supersedes

`memory-bank/tasks/2026-11/261103_cargo-lock-sync.md:37-41` recorded, as the known
trade-off of shipping `release-lock.yml`:

> The known trade-off: a push made with `GITHUB_TOKEN` does not trigger new workflow
> runs, so the release PR's checks do not re-run on the sync commit. That is
> acceptable because those checks require a maintainer to start them anyway (noted
> in `release-please.yml`), which happens after this job lands.

Both halves of that are wrong. The release PR's checks no longer "require a
maintainer to start them anyway" — that approval state was itself caused by the
`GITHUB_TOKEN` push — and the "trade-off" was not a trade-off: because the release
branch was pushed with `GITHUB_TOKEN`, `release-lock.yml` never ran at all, so there
was no sync commit to re-run checks on. The corrected mechanism: release-please
pushes with an App installation token, so `release-lock.yml` fires on the release
branch, and its own sync commit is pushed the same way, so `pr-check.yml` re-runs on
the post-sync head before the release PR is merged.

The body of that historical file is intentionally not rewritten; it now carries
a forward pointer to this record, and this section is the correction of record.

## Verified

Local only — nothing below exercises the credential.

- `actionlint .github/workflows/*.yml` → exit 0 (all nine workflows).
- `node --test scripts/release-workflows.test.mjs` → 13/13 pass.
- The same suite against `HEAD`'s two workflows → 7/13, failing exactly tests 1–4
  (token wiring, unconditional step, scope set, checkout token), 9–10 (the two
  false sentences) and 13 (release-lock's own scope). The docs assertions only pass
  because the docs are already fixed here.
- Negative controls, each run against a throwaway copy of the tree in `/tmp`: an
  `if: false` under release-lock's mint step, deleting that step, replacing its
  `private-key:` with a literal, and deleting its `permission-contents: write` all
  turn the suite red; so do `token: ${{ github.token }}` and
  `…outputs.token || github.token` in `release-please.yml`.
- `git diff --stat -- src-tauri/Cargo.lock` → empty; `Cargo.toml` still `0.11.0`
  and the lock's app entry still `0.10.0`. The drift is intact for AC5 to repair
  through the automation.
- `actions/create-github-app-token` at `v3.3.0` re-resolved to
  `64ab729a6294000f5aa4a4bc0b73d1f32bc45510`.
- No credential-shaped literal anywhere in the diff.

Not verifiable locally: the mint step, the App's permissions, the push event, the
sync commit, and the repair of the 0.11.0/0.10.0 drift.

## Files

**New**

- `scripts/release-workflows.test.mjs` — the 13-case guard, covering both
  workflows
- `memory-bank/tasks/2026-11/261103_release-please-app-token.md`

**Modified**

- `.github/workflows/release-please.yml` — header paragraphs, mint step, `token:`
  on `release-please-action`
- `.github/workflows/release-lock.yml` — header paragraphs, mint step,
  `token:` + explicit `persist-credentials: true` on checkout
- `CONTRIBUTING.md` — three surgical edits: the "derived — next cargo command"
  claim, the mechanism in "The lockfile is gated", and the pointer after step 5
- `docs/BUILD.md` — `### Release automation token`
- `memory-bank/tasks/2026-11/261103_cargo-lock-sync.md` — forward pointer only
  (body unchanged)
- `memory-bank/tasks/2026-11/README.md` — one index entry

**Must show no diff**: `src-tauri/Cargo.lock`, `src-tauri/Cargo.toml`,
`.release-please-manifest.json`, `release-please-config.json`,
`.github/workflows/pr-check.yml`, `.mise.toml`.

## Post-merge runbook

Precondition, before the merge: an App exists and its credentials are stored.
Create `agency-agents-release` (Settings → Developer settings → GitHub Apps → New),
install it on `deuriib/agency-agents-app` only, with no webhook and no OAuth
callback, and grant **Contents: read and write**, **Pull requests: read and
write**, **Issues: read and write**.

```sh
gh variable set RELEASE_APP_CLIENT_ID -R deuriib/agency-agents-app --body "<app client id>"
gh secret set RELEASE_APP_PRIVATE_KEY -R deuriib/agency-agents-app \
  < ~/downloads/release-app.private-key.pem
gh variable list -R deuriib/agency-agents-app --json name --jq '.[].name'
gh secret list   -R deuriib/agency-agents-app --json name --jq '.[].name'
```

Every command below is pinned with `-R deuriib/agency-agents-app`: in this
worktree a bare `gh` resolves to the `upstream` fork.

**AC1** — the token step ran successfully:

```sh
gh workflow run "Release Please" -R deuriib/agency-agents-app --ref main
gh run list -R deuriib/agency-agents-app --workflow "Release Please" --limit 1 \
  --json databaseId,conclusion --jq '.[0]'
gh run view <run-id> -R deuriib/agency-agents-app --json jobs \
  --jq '.jobs[]|select(.name=="Create release PR / tag")|.steps[]|{name,conclusion}'
```

Expected: `Mint the release App token` → `success`, `Run release-please` →
`success`. Failure of the mint step means credentials or App permissions.

**AC2** — `Release lock sync` ran on the release branch (baseline: 0 runs ever):

```sh
# Lifetime count for the workflow, not just the last page of the repo feed.
gh api 'repos/deuriib/agency-agents-app/actions/workflows/release-lock.yml/runs?per_page=1' \
  --jq '.total_count'
gh api 'repos/deuriib/agency-agents-app/actions/workflows/release-lock.yml/runs?per_page=100' \
  --jq '.workflow_runs[]|{id,head_branch,conclusion,head_sha}' | head
```

Expected: `total_count` ≥ 1 with a `release-please--…` head branch, including one
`success` no-op run (the pusher is `cancelled` by design).

**AC3** — the release PR carries the sync commit and is green on that head:

```sh
PR=$(gh pr list -R deuriib/agency-agents-app --label 'autorelease: pending' \
      --json number --jq '.[0].number')
gh pr view "$PR" -R deuriib/agency-agents-app --json commits \
  --jq '.commits[].messageHeadline' | grep 'build(cargo): sync Cargo.lock'
gh pr view "$PR" -R deuriib/agency-agents-app --json headRefOid --jq '.headRefOid'
gh pr view "$PR" -R deuriib/agency-agents-app --json commits \
  --jq '.commits[-1].oid'     # the sync commit must be the head
gh pr checks "$PR" -R deuriib/agency-agents-app \
  | grep -E 'Cargo\.lock matches Cargo\.toml[[:space:]]+pass'
```

If the check is stale, a release-please push landed after the sync; the next
release-lock cycle re-applies it.

**AC4** — `main` is green on the gate after the release PR merges:

```sh
gh run list -R deuriib/agency-agents-app --workflow "PR Check" --branch main --limit 1 \
  --json databaseId,headSha,conclusion --jq '.[0]'
git fetch origin main
git worktree add -q /tmp/main-check origin/main && mise -C /tmp/main-check run check-lock \
  && echo "exit=0" && git worktree remove /tmp/main-check
```

**AC5** — the drift was repaired by the automation, not by hand:

```sh
git show origin/main:src-tauri/Cargo.toml | grep -m1 '^version'
git show origin/main:src-tauri/Cargo.lock | grep -A2 'name = "agency-agents-app"'
git log --oneline -3 origin/main -- src-tauri/Cargo.lock
```

Expected: both versions agree and the newest lockfile commit is
`build(cargo): sync Cargo.lock with X.Y.Z` with no human commit.

**Keep #91 open** until AC2–AC5 print the expected output.

## Follow-ups

- `release-please.yml` still grants `pull-requests: write` and `issues: write` to
  the job's `GITHUB_TOKEN`; after the App is proven, only the narrative-notes step
  uses that token, so both grants are dead. Narrow them in a separate PR.
- `release-lock.yml` keeps `permissions: contents: write` for the `GITHUB_TOKEN`
  even though the App authorizes the push. Same separate least-privilege pass —
  moving it in the commit that changes authentication would have made the
  regression surface wider.
- Do **not** add `permission-workflows: write`. Nothing on the release path touches
  `.github/workflows/`, and that input cannot be narrowed again once granted. If a
  future `extra-files` entry lands there, the push fails loudly naming `workflows`.
- Re-check `gh api repos/deuriib/agency-agents-app/rulesets` after merge: today no
  protection or ruleset exists on `main`, and a future one could block the App push.
- Branch filter is assumed to match
  `release-please--branches--main--components--agency-agents-app`. If the mint step
  succeeds but `Release lock sync` stays at zero runs, suspect the filter second,
  after credentials; do not widen it to `**`, which would run the sync on `main`.
