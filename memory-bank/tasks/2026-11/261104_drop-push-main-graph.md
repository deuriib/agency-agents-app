# 261104_drop-push-main-graph

## Objective

Issue #90. `pr-check.yml` ran all eight jobs on `pull_request` and again on
`push: main`, so every merge repeated the whole graph on a tree that had just been
tested. `gitleaks.yml` duplicated its scan the same way. Measured on the merge of
#95 (run `41754881874`): 7 jobs, 243s of job time, 9 billed minutes, plus a 9s
duplicate secret scan.

## Outcome

- ✅ **Gate first.** `main: PR gate` ruleset (`26500514`) requires the eight
  `pr-check.yml` jobs plus `gitleaks`, with `strict_required_status_checks_policy`,
  and blocks force-pushes and deletion. It went live *before* the workflow change,
  so the PR that removed the duplicate was validated by the rule it depends on.
- ✅ **Then remove.** `rust-lint`, `rust-test` and `frontend` are now
  `pull_request`-only. `rust-fmt`, `cargo-lock`, `scripts` and `vendor` keep
  running on `push: main` as a canary.
- ✅ **`gitleaks`** drops `push: [main]`; `pull_request`, the weekly `schedule` and
  `workflow_dispatch` stay.
- ✅ **`release-please.yml`** untouched — `push: main` is its trigger, not a
  duplicate. `pages.yml` and `sync-labels.yml` are path-filtered deploys.
- ✅ Documented in `CONTRIBUTING.md#main-is-gated`, with the note the issue asked
  for: do not re-add `push: main` to the full graph without reading #90.

## Measured

| | before | after | delta |
|---|---|---|---|
| jobs queued on `push: main` | 7 (+1 gitleaks) | 4 | −4 |
| job time | 252s | 39s | **−213s (−85%)** |
| billed minutes | 10 | 4 | **−6 (−60%)** |
| wall clock | ~2m20s | 15s | **−2m05s** |

Before: run `41754881874` (push, merge of #95, `5234cca`) + `41754881875`.
After: run `41759214920` (push, `d186253`); `Secret Scan` absent from the push
entirely — the workflow did not trigger, it did not "skip".

## Why the split is sound

A squash merge of an up-to-date head produces a tree **identical** to the head CI
already tested — same code, new SHA — so re-running the compile jobs on the merge
commit cannot yield a different verdict. Two repository settings make that a
guarantee rather than a hope, and both were already true or were set here:

- `allow_merge_commit: false`, `allow_rebase_merge: false` — squash is the only
  merge strategy, so the merged tree is the head tree.
- `strict_required_status_checks_policy: true` — a PR behind `main` cannot merge
  until updated.

The second one alone would have been a trap: `allow_update_branch` was `false`, so
"update your branch" meant a local rebase plus force-push every time. Enabled, it
is the **Update branch** button. A gate that is annoying is a gate that gets
switched off within a month — which is the failure mode the "do not re-add" note
exists to prevent.

## What the canary is actually for

With the gate closed, no merge can produce an untested tree through the normal
path. The canary covers the abnormal ones: a ruleset edit, a restored ref, an
administrator override. It fails in seconds instead of at the next release, and it
costs four jobs that read manifests and pinned artifacts — no compile, no
`npm ci`.

A job added to `pr-check.yml` without an `if:` simply runs on both events. That is
the old, safe behaviour, not a broken one: the failure mode of forgetting the guard
is redundancy, not a hole.

## Evidenced, not asserted

**A red required check blocks the merge.** Probe PR #100 appended one deliberately
unformatted function to `src-tauri/src/lib.rs`; `Rust formatting (cargo fmt
--check)` failed while the other eight passed, `mergeStateStatus` went `BLOCKED`,
and `gh pr merge --squash` returned `the base branch policy prohibits the merge`.
Closed unmerged.

**A direct push cannot sidestep it.** `git push --dry-run origin HEAD:main`
reported success and proved nothing — git never reaches the server, so the ruleset
is not evaluated. Instead a sacrificial ruleset with the same
`required_status_checks` rule was pointed at a throwaway `gate-probe` ref and an
untested commit was pushed at it:

```
remote: error: GH013: Repository rule violations found for refs/heads/gate-probe.
remote: - Required status check "Rust tests" is expected.
 ! [remote rejected] HEAD -> gate-probe (push declined due to repository rule violations)
```

Probe ruleset and ref deleted; the ref never existed.

## What the gate is not

Not unskippable, and `CONTRIBUTING.md` says so. No `bypass_actors` are declared,
which closes the normal path. A repository admin can still edit or delete the
ruleset, or use the API administrator override — `gh pr merge` advertises
`--admin` on every blocked merge. The value is that skipping becomes a deliberate,
visible act instead of a quiet default. Documenting an absolute claim that the
first curious maintainer can disprove would cost the document its authority.

Each required context is pinned to `integration_id: 15368` (GitHub Actions). The
check name alone is not enough: without the pin, a third-party App could publish a
green check run called `Rust tests` over a red PR.

## Files

**Modified**

- `.github/workflows/pr-check.yml` — three jobs gated to `pull_request`, header
  comment rewritten to record the reasoning next to the trigger
- `.github/workflows/gitleaks.yml` — `push: [main]` removed, header updated
- `CONTRIBUTING.md` — new **Main is gated** section under Tests

**Repository settings** (not in the tree)

- ruleset `main: PR gate` = `26500514`
- `allow_update_branch` → `true`

## Follow-ups

- Issue #90's acceptance criterion 2 says `pr-check.yml` "no longer triggers on
  `push`", while its proposal 4 asks for a canary *on* `push: main`. Proposal 4 was
  taken, reading criterion 2 as "the graph no longer runs on push". Honouring it
  literally needs the canary in its own workflow, which duplicates the `cargo-lock`
  and `vendor` steps and then drifts silently. Deferred deliberately.
- `gitleaks` is a ninth required check, beyond the issue's list of eight. Without
  it, dropping `push: [main]` would leave a secret on `main` unseen until Monday's
  cron — the cleanup, not the gate.
- This repository is a public fork, so none of these minutes were billed. For
  scale: `Release Please` on the same push still costs 760s of job time (Windows
  479s, Linux 241s). `PR Check` was 24% of the post-merge cost and is now 5%.
