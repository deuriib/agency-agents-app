import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { TARGETS, buildManifest, parseArgs } from "./updater-manifest.mjs";

const BASE_URL = "https://github.com/deuriib/agency-agents-app/releases/download/v0.8.0";
const SIGNATURE = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==";

function workspace(t) {
  const dir = mkdtempSync(join(tmpdir(), "updater-manifest-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const assets = join(dir, "assets");
  mkdirSync(assets);
  return { dir, assets };
}

function addArtifact(assetsDir, filename, signature = SIGNATURE) {
  writeFileSync(join(assetsDir, filename), "artifact bytes");
  writeFileSync(join(assetsDir, `${filename}.sig`), `${signature}\n`);
}

const base = {
  version: "0.8.0",
  notes: "Release notes.",
  pubDate: "2026-11-02T12:00:00Z",
  baseUrl: BASE_URL,
};

const MAC_AARCH64 = "Agency.Agents_0.8.0_aarch64.app.tar.gz";
const MAC_X64 = "Agency.Agents_0.8.0_x64.app.tar.gz";
const WIN_X64 = "Agency.Agents_0.8.0_x64-setup.exe";

test("TARGETS is the exact Tauri v2 allowlist", () => {
  assert.deepEqual([...TARGETS], [
    "darwin-aarch64",
    "darwin-x86_64",
    "windows-x86_64",
    "windows-aarch64",
    "linux-x86_64",
  ]);
  assert.ok(Object.isFrozen(TARGETS));
});

test("builds a manifest for a single platform", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  const manifest = buildManifest({
    ...base,
    assetsDir: assets,
    assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
  });

  assert.deepEqual(manifest, {
    version: "0.8.0",
    notes: "Release notes.",
    pub_date: "2026-11-02T12:00:00Z",
    platforms: {
      "darwin-aarch64": {
        signature: SIGNATURE,
        url: `${BASE_URL}/${MAC_AARCH64}`,
      },
    },
  });
  assert.deepEqual(Object.keys(manifest), ["version", "notes", "pub_date", "platforms"]);
});

test("builds multiple platforms sorted by target key", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, WIN_X64);
  addArtifact(assets, MAC_X64);
  addArtifact(assets, MAC_AARCH64);

  const manifest = buildManifest({
    ...base,
    assetsDir: assets,
    // Deliberately out of order: output must be deterministic.
    assets: [
      { target: "windows-x86_64", filename: WIN_X64 },
      { target: "darwin-x86_64", filename: MAC_X64 },
      { target: "darwin-aarch64", filename: MAC_AARCH64 },
    ],
  });

  assert.deepEqual(Object.keys(manifest.platforms), [
    "darwin-aarch64",
    "darwin-x86_64",
    "windows-x86_64",
  ]);
  assert.equal(manifest.platforms["windows-x86_64"].url, `${BASE_URL}/${WIN_X64}`);
});

test("encodes reserved characters in the artifact filename", (t) => {
  const { assets } = workspace(t);
  const filename = "Agency Agents (arm64)+signed.app.tar.gz";
  addArtifact(assets, filename);

  const manifest = buildManifest({
    ...base,
    assetsDir: assets,
    assets: [{ target: "darwin-aarch64", filename }],
  });

  assert.equal(
    manifest.platforms["darwin-aarch64"].url,
    `${BASE_URL}/Agency%20Agents%20(arm64)%2Bsigned.app.tar.gz`,
  );
});

test("keeps the signature verbatim apart from the trailing newline", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64, "  spaced signature  ");

  const manifest = buildManifest({
    ...base,
    assetsDir: assets,
    assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
  });

  assert.equal(manifest.platforms["darwin-aarch64"].signature, "  spaced signature  ");
});

test("strips a leading v from the version", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  const manifest = buildManifest({
    ...base,
    version: "v0.8.0",
    assetsDir: assets,
    assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
  });

  assert.equal(manifest.version, "0.8.0");
});

test("accepts a prerelease version", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  const manifest = buildManifest({
    ...base,
    version: "0.9.0-beta.2",
    assetsDir: assets,
    assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
  });

  assert.equal(manifest.version, "0.9.0-beta.2");
});

test("omits notes and pub_date when they are not supplied", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  const manifest = buildManifest({
    version: "0.8.0",
    baseUrl: BASE_URL,
    assetsDir: assets,
    assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
  });

  assert.deepEqual(Object.keys(manifest), ["version", "platforms"]);
  assert.equal("notes" in manifest, false);
  assert.equal("pub_date" in manifest, false);
});

test("reads notes from --notes-file", (t) => {
  const { dir, assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);
  const notesPath = join(dir, "notes.md");
  writeFileSync(notesPath, "## What's new\n\n- fixed the updater\n");

  const manifest = buildManifest({
    ...base,
    notes: undefined,
    notesFile: notesPath,
    assetsDir: assets,
    assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
  });

  assert.equal(manifest.notes, "## What's new\n\n- fixed the updater");
});

test("rejects --notes together with --notes-file", (t) => {
  const { dir, assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);
  const notesPath = join(dir, "notes.md");
  writeFileSync(notesPath, "notes");

  assert.throws(
    () =>
      buildManifest({
        ...base,
        notesFile: notesPath,
        assetsDir: assets,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /mutually exclusive/,
  );
});

test("merge preserves platforms that are not overridden", (t) => {
  const { dir, assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);
  const mergePath = join(dir, "latest.json");
  writeFileSync(
    mergePath,
    `${JSON.stringify(
      {
        version: "0.7.1",
        platforms: {
          "linux-x86_64": {
            signature: "bGludXg=",
            url: "https://github.com/deuriib/agency-agents-app/releases/download/v0.7.1/Agency.Agents_0.7.1_amd64.AppImage.tar.gz",
          },
          "windows-x86_64": {
            signature: "d2luZG93cw==",
            url: "https://github.com/deuriib/agency-agents-app/releases/download/v0.7.1/Agency.Agents_0.7.1_x64-setup.exe",
          },
        },
      },
      null,
      2,
    )}\n`,
  );

  const manifest = buildManifest({
    ...base,
    assetsDir: assets,
    merge: mergePath,
    assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
  });

  assert.deepEqual(Object.keys(manifest.platforms), [
    "darwin-aarch64",
    "linux-x86_64",
    "windows-x86_64",
  ]);
  assert.equal(manifest.platforms["linux-x86_64"].signature, "bGludXg=");
  assert.equal(manifest.version, "0.8.0");
});

test("an asset overrides the merged entry for the same target", (t) => {
  const { dir, assets } = workspace(t);
  addArtifact(assets, MAC_X64);
  const mergePath = join(dir, "latest.json");
  writeFileSync(
    mergePath,
    `${JSON.stringify({
      version: "0.7.1",
      platforms: {
        "darwin-x86_64": {
          signature: "b2xk",
          url: "https://example.com/old/Agency.Agents_0.7.1_x64.app.tar.gz",
        },
      },
    })}\n`,
  );

  const manifest = buildManifest({
    ...base,
    assetsDir: assets,
    merge: mergePath,
    assets: [{ target: "darwin-x86_64", filename: MAC_X64 }],
  });

  assert.deepEqual(manifest.platforms["darwin-x86_64"], {
    signature: SIGNATURE,
    url: `${BASE_URL}/${MAC_X64}`,
  });
});

test("a missing merge file means nothing to merge yet", (t) => {
  const { dir, assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  const manifest = buildManifest({
    ...base,
    assetsDir: assets,
    merge: join(dir, "does-not-exist.json"),
    assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
  });

  assert.deepEqual(Object.keys(manifest.platforms), ["darwin-aarch64"]);
});

test("rejects invalid JSON in the merge file", (t) => {
  const { dir, assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);
  const mergePath = join(dir, "latest.json");
  writeFileSync(mergePath, "{ not json");

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        merge: mergePath,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /invalid JSON/,
  );
});

test("rejects an unknown platform in the merge file", (t) => {
  const { dir, assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);
  const mergePath = join(dir, "latest.json");
  writeFileSync(
    mergePath,
    `${JSON.stringify({
      version: "0.7.1",
      platforms: { "darwin-arm64": { signature: "a", url: "https://example.com/a" } },
    })}\n`,
  );

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        merge: mergePath,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /unknown platform "darwin-arm64"/,
  );
});

test("rejects an http url inside the merge file", (t) => {
  const { dir, assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);
  const mergePath = join(dir, "latest.json");
  writeFileSync(
    mergePath,
    `${JSON.stringify({
      version: "0.7.1",
      platforms: { "linux-x86_64": { signature: "a", url: "http://example.com/a" } },
    })}\n`,
  );

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        merge: mergePath,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /must be https/,
  );
});

test("rejects a missing signature file", (t) => {
  const { assets } = workspace(t);
  writeFileSync(join(assets, MAC_AARCH64), "artifact bytes");

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /Signature not found/,
  );
});

test("rejects an empty signature file", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64, "");

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /Signature is empty/,
  );
});

test("rejects an unknown target", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        assets: [{ target: "darwin-arm64", filename: MAC_AARCH64 }],
      }),
    /Unknown target "darwin-arm64"/,
  );
});

test("rejects a duplicate target", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);
  addArtifact(assets, MAC_X64);

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        assets: [
          { target: "darwin-aarch64", filename: MAC_AARCH64 },
          { target: "darwin-aarch64", filename: MAC_X64 },
        ],
      }),
    /Duplicate --asset for target "darwin-aarch64"/,
  );
});

test("rejects a missing artifact", (t) => {
  const { assets } = workspace(t);

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /Artifact not found/,
  );
});

test("rejects a filename that is a path", (t) => {
  const { dir, assets } = workspace(t);
  writeFileSync(join(dir, "outside.txt"), "x");
  writeFileSync(join(dir, "outside.txt.sig"), "sig\n");

  assert.throws(
    () =>
      buildManifest({
        ...base,
        assetsDir: assets,
        assets: [{ target: "linux-x86_64", filename: "../outside.txt" }],
      }),
    /bare file name/,
  );
});

test("rejects a non-https base url", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  assert.throws(
    () =>
      buildManifest({
        ...base,
        baseUrl: "http://example.com/releases/v0.8.0",
        assetsDir: assets,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /must be https/,
  );
});

test("rejects a base url with a query string", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  assert.throws(
    () =>
      buildManifest({
        ...base,
        baseUrl: "https://example.com/releases?tag=v0.8.0",
        assetsDir: assets,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /without a query or fragment/,
  );
});

test("rejects a malformed base url", () => {
  assert.throws(
    () => buildManifest({ ...base, baseUrl: "not a url", assets: [] }),
    /is not a valid URL/,
  );
});

test("rejects an invalid semver", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  assert.throws(
    () =>
      buildManifest({
        ...base,
        version: "0.8",
        assetsDir: assets,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /is not a semver/,
  );
});

test("rejects a missing version", () => {
  assert.throws(() => buildManifest({ ...base, version: undefined, assets: [] }), /--version is required/);
});

test("rejects a missing base url", () => {
  assert.throws(() => buildManifest({ ...base, baseUrl: undefined, assets: [] }), /--base-url is required/);
});

test("rejects an invalid pub_date", (t) => {
  const { assets } = workspace(t);
  addArtifact(assets, MAC_AARCH64);

  assert.throws(
    () =>
      buildManifest({
        ...base,
        pubDate: "2026-11-02 12:00:00",
        assetsDir: assets,
        assets: [{ target: "darwin-aarch64", filename: MAC_AARCH64 }],
      }),
    /is not an RFC3339 timestamp/,
  );
});

test("rejects an empty platform list", () => {
  assert.throws(
    () => buildManifest({ version: "0.8.0", baseUrl: BASE_URL, assets: [] }),
    /No platforms to write/,
  );
});

test("parseArgs collects repeatable assets and equals-form flags", () => {
  const args = parseArgs([
    "--version=v0.8.0",
    "--base-url",
    BASE_URL,
    "--assets-dir",
    "dist",
    "--asset",
    "darwin-aarch64=a.app.tar.gz",
    "--asset=darwin-x86_64=b.app.tar.gz",
    "--merge",
    "latest.json",
    "--out",
    "-",
  ]);

  assert.equal(args.version, "v0.8.0");
  assert.equal(args.baseUrl, BASE_URL);
  assert.equal(args.assetsDir, "dist");
  assert.deepEqual(args.assets, [
    { target: "darwin-aarch64", filename: "a.app.tar.gz" },
    { target: "darwin-x86_64", filename: "b.app.tar.gz" },
  ]);
  assert.equal(args.merge, "latest.json");
  assert.equal(args.out, "-");
  assert.equal(args.help, false);
});

test("parseArgs reports unknown options", () => {
  assert.throws(() => parseArgs(["--nope", "x"]), /Unknown option "--nope"/);
});

test("parseArgs reports a missing value", () => {
  assert.throws(() => parseArgs(["--version"]), /Option "--version" requires a value/);
  assert.throws(() => parseArgs(["--version", "--out", "x"]), /Option "--version" requires a value/);
});

test("parseArgs reports duplicate single-value options", () => {
  assert.throws(
    () => parseArgs(["--version", "1.0.0", "--version", "2.0.0"]),
    /Duplicate option "--version"/,
  );
});

test("parseArgs reports malformed --asset values", () => {
  assert.throws(() => parseArgs(["--asset", "darwin-aarch64"]), /must be <target>=<filename>/);
  assert.throws(() => parseArgs(["--asset", "=a.tar.gz"]), /must be <target>=<filename>/);
  assert.throws(() => parseArgs(["--asset", "darwin-aarch64="]), /must be <target>=<filename>/);
});

test("parseArgs reports stray positional arguments", () => {
  assert.throws(() => parseArgs(["darwin-aarch64"]), /Unexpected argument "darwin-aarch64"/);
});

test("parseArgs recognises help", () => {
  assert.equal(parseArgs(["--help"]).help, true);
  assert.equal(parseArgs(["-h"]).help, true);
});
