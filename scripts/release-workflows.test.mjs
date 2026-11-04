/**
 * Guards the release automation against the class of bug in issue #91.
 *
 * `release-please` pushed its release branch with `GITHUB_TOKEN`, and a
 * `GITHUB_TOKEN` push starts no workflow run at all, so `release-lock.yml` never
 * fired and `src-tauri/Cargo.lock` stayed one release behind while every job
 * stayed green. These tests pin the fix: both workflows mint a GitHub App
 * installation token, use it for the push, and fail rather than fall back to
 * `GITHUB_TOKEN` — plus the docs that tell a maintainer how to rotate the key.
 *
 * What this file does NOT test: that the version in `src-tauri/Cargo.toml` equals
 * the one in `src-tauri/Cargo.lock`. That invariant is owned by the `cargo-lock`
 * job (`mise run check-lock`, `cargo metadata --locked`); asserting a second,
 * hand-parsed copy here would make this file a weaker second source of truth.
 *
 * Everything is plain text parsing — no YAML parser, no dependencies, no network.
 * The block helpers assume block-style YAML with `- name:` steps.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dir = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(join(dir, path), "utf8");

const workflowsDir = ".github/workflows";
const workflows = new Map(
  readdirSync(join(dir, workflowsDir))
    // `.yaml` counts too: a filter on `.yml` alone lets a new spelling of the
    // same directory bypass every structural assertion below.
    .filter((name) => /\.ya?ml$/.test(name))
    .map((name) => [name, read(join(workflowsDir, name))]),
);

const releasePlease = read(`${workflowsDir}/release-please.yml`);
const releaseLock = read(`${workflowsDir}/release-lock.yml`);
const contributing = read("CONTRIBUTING.md");
const buildDoc = read("docs/BUILD.md");

// Both release workflows must mint and consume their own App token; neither may
// fall back to `GITHUB_TOKEN`, and neither may ask for a scope it does not use.
const authenticatedWorkflows = [
  ["release-please.yml", releasePlease],
  ["release-lock.yml", releaseLock],
];

const indentOf = (line) => line.length - line.trimStart().length;

/**
 * The block-style list item (`- name: …`) that contains `marker`, sliced from its
 * own dash to the first non-blank line that is not more indented than the dash.
 * The item's own indentation is what makes `if:` detection exact: a shell
 * `if [[ … ]]` inside a `run:` body sits deeper and cannot be mistaken for it.
 */
function itemBlock(source, marker) {
  const lines = source.split("\n");
  const markerAt = lines.findIndex((line) => line.includes(marker));
  assert.notEqual(markerAt, -1, `no step contains ${marker}`);

  let start = -1;
  for (let i = markerAt; i >= 0; i -= 1) {
    if (/^\s*- /.test(lines[i])) {
      start = i;
      break;
    }
  }
  assert.notEqual(start, -1, `${marker} is not inside a list item`);

  const itemIndent = indentOf(lines[start]);
  const block = [];
  for (let i = start; i < lines.length; i += 1) {
    const line = lines[i];
    if (i > start && line.trim() !== "" && indentOf(line) <= itemIndent) break;
    block.push(line);
  }
  return block.join("\n").trimEnd();
}

/**
 * A job mapping sliced from its key until indentation drops back out of the job.
 * Needed because walking back from a job's `name:` would otherwise land on the
 * `- "release-please--**"` item in the `on:` block.
 */
function keyBlock(source, key, indent) {
  const lines = source.split("\n");
  const start = lines.indexOf(`${" ".repeat(indent)}${key}`);
  assert.notEqual(start, -1, `${key} must exist at indent ${indent}`);

  const block = [];
  for (let i = start; i < lines.length; i += 1) {
    const line = lines[i];
    if (i > start && line.trim() !== "" && indentOf(line) <= indent) break;
    block.push(line);
  }
  return block.join("\n").trimEnd();
}

/** Every line that lives inside a `run: |` / `run: >` script body. */
function runBodies(source) {
  const lines = source.split("\n");
  const bodies = [];
  for (let i = 0; i < lines.length; i += 1) {
    const header = /^(\s*)(?:-\s+)?run:\s*[|>][-+]?\s*$/.exec(lines[i]);
    if (!header) continue;
    const runIndent = header[1].length;
    for (let j = i + 1; j < lines.length; j += 1) {
      const line = lines[j];
      if (line.trim() !== "" && indentOf(line) <= runIndent) break;
      bodies.push(line);
    }
  }
  return bodies;
}

test("release-please authenticates release-please-action with the App token, not GITHUB_TOKEN", () => {
  const step = itemBlock(releasePlease, "uses: googleapis/release-please-action@");
  const token = /^\s*token:\s*(.+)$/m.exec(step);

  assert.ok(token, "the release-please step must pass a token explicitly");
  assert.match(
    token[1].trim(),
    /^\$\{\{\s*steps\.[\w-]+\.outputs\.token\s*\}\}$/,
    "the token must come from a step output, not a literal or a secret name",
  );
  assert.doesNotMatch(
    token[1],
    /\|\|/,
    "a fallback (even `|| github.token`) is the exact mechanism that let #91 ship green",
  );
});

test("the token step runs unconditionally and is referenced by id", () => {
  for (const [file, source] of authenticatedWorkflows) {
    const step = itemBlock(source, "uses: actions/create-github-app-token@");
    const idLine = step.split("\n").find((line) => /^\s*id:\s*\S+\s*$/.test(line));
    assert.ok(idLine, `${file}: the mint step must expose an id`);

    // Step keys sit two columns right of the `- `, so `if:` would align with `id:`,
    // not with the dash. Measuring at the dash would miss a re-added conditional.
    const keyIndent = indentOf(idLine);
    assert.doesNotMatch(
      step,
      new RegExp(`^ {${keyIndent}}if:`, "m"),
      `${file}: a skipped token step is indistinguishable from a working one until the release path never syncs`,
    );

    const id = /^\s*id:\s*(\S+)\s*$/.exec(idLine)[1];
    assert.match(
      source,
      new RegExp(`steps\\.${id}\\.outputs\\.token`),
      `${file}: nothing consumes the token this step mints`,
    );

    // Exact lines, not a name count: deleting the credential reference must not
    // pass by leaving less to match.
    assert.match(
      step,
      /^\s*client-id:\s*\$\{\{\s*vars\.RELEASE_APP_CLIENT_ID\s*\}\}$/m,
      `${file}: client-id must read the App client ID variable`,
    );
    assert.match(
      step,
      /^\s*private-key:\s*\$\{\{\s*secrets\.RELEASE_APP_PRIVATE_KEY\s*\}\}$/m,
      `${file}: private-key must read the App private key secret`,
    );
  }
});

test("the minted token is downscoped to contents, pull-requests and issues", () => {
  const step = itemBlock(releasePlease, "uses: actions/create-github-app-token@");
  const scopes = [...step.matchAll(/^\s*permission-([a-z-]+):\s*(\S+)\s*$/gm)]
    .map((match) => `${match[1]}=${match[2]}`)
    .sort();

  assert.deepEqual(
    scopes,
    ["contents=write", "issues=write", "pull-requests=write"],
    "the exact scope set, so both a missing and an added permission fail here",
  );
  assert.match(step, /^\s*client-id:\s*\$\{\{\s*vars\.RELEASE_APP_CLIENT_ID\s*\}\}$/m);
  assert.match(step, /^\s*private-key:\s*\$\{\{\s*secrets\.RELEASE_APP_PRIVATE_KEY\s*\}\}$/m);
});

test("release-lock pushes with the App token and keeps contents: write", () => {
  const job = keyBlock(releaseLock, "sync:", 2);
  const checkout = itemBlock(job, "uses: actions/checkout@");

  assert.match(
    checkout,
    /^\s*token:\s*\$\{\{\s*steps\.[\w-]+\.outputs\.token\s*\}\}$/m,
    "checkout is what writes the credential into the local git config, so the push authenticates as the App",
  );
  assert.match(job, /^\s+permissions:\n\s+contents: write$/m);
  assert.match(
    job,
    /GitHub App|App token/,
    "the reason for the token belongs beside the token, not in a distant comment",
  );
});

test("no workflow echoes the minted token or the private key", () => {
  for (const [file, source] of workflows) {
    assert.doesNotMatch(
      source,
      /^\s*-?\s*run:[^\n|>]*(outputs\.token|RELEASE_APP_PRIVATE_KEY)/m,
      `${file}: a one-line run: must never interpolate the credential`,
    );

    const leaking = runBodies(source).filter(
      (line) => line.includes("outputs.token") || line.includes("RELEASE_APP_PRIVATE_KEY"),
    );
    assert.deepEqual(leaking, [], `${file}: a run: script body must never read the credential`);
  }
});

test("no credential literal is committed", () => {
  const shaped = /ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY-----/;
  const documents = new Map([
    ["CONTRIBUTING.md", contributing],
    ["docs/BUILD.md", buildDoc],
  ]);

  for (const [file, source] of [...workflows, ...documents]) {
    assert.doesNotMatch(source, shaped, `${file} contains something shaped like a credential`);
  }
});

test("the private key is only ever a secret expression", () => {
  const canonical = /^\s*private-key:\s*\$\{\{\s*secrets\.RELEASE_APP_PRIVATE_KEY\s*\}\}\s*$/;

  for (const [file, source] of workflows) {
    for (const line of source.split("\n")) {
      if (!line.includes("RELEASE_APP_PRIVATE_KEY")) continue;
      assert.match(line, canonical, `${file}: ${line.trim()}`);
    }
  }
});

test("every non-local action is pinned to a 40-hex commit with a ref comment", () => {
  for (const [file, source] of workflows) {
    source.split("\n").forEach((line, index) => {
      // Header prose legitimately starts a line with "# uses: …".
      if (/^\s*#/.test(line)) return;

      const declared = /^\s*(?:-\s+)?uses:\s*(\S+)/.exec(line);
      if (!declared) return;

      const target = declared[1];
      // A local reusable workflow cannot carry a SHA — that is a syntax error.
      if (target.startsWith("./")) return;

      assert.match(
        target,
        /@[0-9a-f]{40}$/,
        `${file}:${index + 1} ${target} must pin a full 40-hex commit`,
      );
      // Any ref comment, not a semver one: `dtolnay/rust-toolchain` pins `# master`.
      assert.match(line, /\s#\s*\S+/, `${file}:${index + 1} ${target} needs a ref comment`);
    });
  }
});

test("release-please.yml no longer claims no token is needed", () => {
  assert.doesNotMatch(releasePlease, /No PAT or app token is needed/);
  assert.doesNotMatch(releasePlease, /next cargo command/);
  assert.match(releasePlease, /docs\/BUILD\.md#release-automation-token/);
});

test("release-lock.yml no longer claims its checks need a maintainer", () => {
  assert.doesNotMatch(releaseLock, /require a maintainer to start them anyway/);
});

test("CONTRIBUTING.md points at the automation, not a local command", () => {
  assert.doesNotMatch(contributing, /next cargo command/);
  assert.match(contributing, /docs\/BUILD\.md#release-automation-token/);
});

test("docs/BUILD.md documents both names, the permissions and rotation", () => {
  const heading = "### Release automation token";
  const at = buildDoc.indexOf(heading);
  assert.notEqual(at, -1, "`#release-automation-token` depends on this exact heading");

  const section = buildDoc.slice(at);
  assert.match(section, /RELEASE_APP_CLIENT_ID/);
  assert.match(section, /RELEASE_APP_PRIVATE_KEY/);
  assert.match(section, /gh secret set RELEASE_APP_PRIVATE_KEY/);
  assert.match(section, /Private keys/);
  assert.match(section, /rotat/i);
});

test("release-lock narrows its token to contents: write alone", () => {
  const step = itemBlock(releaseLock, "uses: actions/create-github-app-token@");
  const scopes = [...step.matchAll(/^\s*permission-([a-z-]+):\s*(\S+)\s*$/gm)]
    .map((match) => `${match[1]}=${match[2]}`)
    .sort();

  assert.deepEqual(
    scopes,
    ["contents=write"],
    "the lock sync only commits and pushes one file, so any wider scope is unreviewed authority",
  );
});
