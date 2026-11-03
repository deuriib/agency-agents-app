/**
 * Guards `scripts/release.sh` against hardcoded release identity.
 *
 * The script used to `export APPLE_ID="<someone>@mac.com"` unconditionally, so a
 * fork could not notarize with its own Apple account — the export clobbered any
 * value the caller set. These tests pin the two properties that fixed it: the
 * identity comes from the environment, and the script refuses to run without it.
 *
 * They run on any platform: the validation exits before the script touches
 * `security` or `xcrun`.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const script = join(here, "release.sh");

function withoutIdentity() {
  const env = { ...process.env };
  delete env.APPLE_ID;
  delete env.APPLE_TEAM_ID;
  delete env.NOTARY_PROFILE;
  return env;
}

test("release.sh refuses to run without an Apple identity", () => {
  const run = spawnSync("bash", [script], {
    env: withoutIdentity(),
    encoding: "utf8",
  });

  assert.equal(run.status, 1, "it must exit non-zero, not fall through");
  assert.match(run.stderr, /Missing Apple Developer identity/);
  assert.match(run.stderr, /APPLE_ID is unset/);
  assert.match(run.stderr, /APPLE_TEAM_ID is unset/);
});

test("release.sh names only the variable that is missing", () => {
  const run = spawnSync("bash", [script], {
    env: { ...withoutIdentity(), APPLE_ID: "someone@example.com" },
    encoding: "utf8",
  });

  assert.equal(run.status, 1);
  assert.doesNotMatch(
    run.stderr,
    /APPLE_ID is unset/,
    "APPLE_ID was provided, so it must not be reported as missing",
  );
  assert.match(run.stderr, /APPLE_TEAM_ID is unset/);
});

test("release.sh takes the identity from the environment", () => {
  const source = readFileSync(script, "utf8");
  // Comments legitimately show an example; only executable lines matter.
  const code = source
    .split("\n")
    .filter((line) => !line.trimStart().startsWith("#"))
    .join("\n");

  assert.match(code, /export APPLE_ID="\$\{APPLE_ID:-\}"/);
  assert.match(code, /export APPLE_TEAM_ID="\$\{APPLE_TEAM_ID:-\}"/);
  assert.doesNotMatch(
    code,
    /APPLE_(ID|TEAM_ID)="[^"$]/,
    "a literal identity assignment would clobber the caller's value",
  );
});

test("release.sh carries no personal email", () => {
  const source = readFileSync(script, "utf8");
  const emails = source.match(/[\w.+-]+@[\w-]+\.[\w.]+/g) ?? [];
  const personal = emails.filter((email) => !email.endsWith("@example.com"));

  assert.deepEqual(personal, [], `unexpected personal address: ${personal.join(", ")}`);
});
