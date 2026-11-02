import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  TARGETS,
  loadManifest,
  parseArgs,
  targetUrl,
  validateManifest,
} from "./verify-updater-manifest.mjs";

const SIGNATURE = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==";
const BASE_URL =
  "https://github.com/deuriib/agency-agents-app/releases/download/v0.8.0";

function entry(file) {
  return { signature: SIGNATURE, url: `${BASE_URL}/${file}` };
}

/** A complete manifest with every platform Tauri knows about. */
function fullManifest() {
  return {
    version: "0.8.0",
    notes: "Release notes.",
    pub_date: "2026-11-02T12:00:00Z",
    platforms: {
      "darwin-aarch64": entry("Agency.Agents_0.8.0_aarch64.app.tar.gz"),
      "darwin-x86_64": entry("Agency.Agents_0.8.0_x64.app.tar.gz"),
      "linux-x86_64": entry("Agency.Agents_0.8.0_amd64.AppImage.tar.gz"),
      "windows-x86_64": entry("Agency.Agents_0.8.0_x64-setup.exe"),
      "windows-aarch64": entry("Agency.Agents_0.8.0_arm64-setup.exe"),
    },
  };
}

test("targetUrl maps the two channels to the URLs the app fetches", () => {
  assert.equal(
    targetUrl("stable"),
    "https://github.com/deuriib/agency-agents-app/releases/latest/download/latest.json",
  );
  assert.equal(
    targetUrl("beta"),
    "https://github.com/deuriib/agency-agents-app/releases/download/updater-beta/latest.json",
  );
  assert.throws(() => targetUrl("nightly"), /unknown channel/);
});

test("a complete manifest passes and platforms are sorted", () => {
  const result = validateManifest(fullManifest());
  assert.equal(result.ok, true);
  assert.deepEqual(result.errors, []);
  assert.deepEqual(
    result.platforms.map((platform) => platform.target),
    [...TARGETS].sort((a, b) => a.localeCompare(b)),
  );
  assert.ok(result.platforms.every((platform) => platform.ok));
});

test("version must be valid semver", () => {
  for (const version of ["0.8", "v0.8.0", "0.8.0-", "1.02.3", ""]) {
    const manifest = { ...fullManifest(), version };
    const result = validateManifest(manifest);
    assert.equal(result.ok, false, `expected ${JSON.stringify(version)} to fail`);
    assert.ok(result.errors.some((error) => error.includes("version")));
  }
});

test("missing and non-object platforms are rejected", () => {
  const missing = { ...fullManifest(), platforms: undefined };
  assert.equal(validateManifest(missing).ok, false);

  const array = { ...fullManifest(), platforms: [] };
  assert.equal(validateManifest(array).ok, false);
});

test("unknown platform keys are rejected instead of ignored", () => {
  const manifest = fullManifest();
  manifest.platforms["darwin-arm64"] = entry("wrong-key");
  const result = validateManifest(manifest);
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((error) => error.includes("darwin-arm64")));
});

test("empty signatures and non-https URLs are rejected", () => {
  const manifest = fullManifest();
  manifest.platforms["linux-x86_64"].signature = "   ";
  manifest.platforms["windows-x86_64"].url =
    "http://github.com/deuriib/agency-agents-app/releases/download/x";

  const result = validateManifest(manifest);
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((error) => error.includes("signature is empty")));
  assert.ok(result.errors.some((error) => error.includes("not an https URL")));
});

test("missing expected platforms are reported", () => {
  const manifest = fullManifest();
  delete manifest.platforms["windows-aarch64"];
  const result = validateManifest(manifest);
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((error) => error.includes("windows-aarch64")));
});

test("allowSubset accepts a partial manifest but still validates entries", () => {
  const manifest = {
    version: "0.8.0",
    platforms: { "darwin-aarch64": entry("only-mac.app.tar.gz") },
  };
  assert.equal(validateManifest(manifest, { expected: [] }).ok, true);
  assert.equal(validateManifest(manifest).ok, false);
});

test("parseArgs defaults to the stable channel", () => {
  const options = parseArgs([]);
  assert.equal(options.channel, "stable");
  assert.equal(options.checkUrls, false);
  assert.equal(options.allowSubset, false);
  assert.equal(options.json, false);
  assert.equal(options.url, null);
  assert.equal(options.file, null);
});

test("parseArgs accepts channel, expect, url, file and booleans", () => {
  const options = parseArgs([
    "--channel",
    "beta",
    "--expect",
    "linux-x86_64, windows-x86_64",
    "--check-urls",
    "--json",
  ]);
  assert.equal(options.channel, "beta");
  assert.deepEqual(options.expect, ["linux-x86_64", "windows-x86_64"]);
  assert.equal(options.checkUrls, true);
  assert.equal(options.json, true);

  assert.equal(parseArgs(["--url=https://example.test/latest.json"]).url, "https://example.test/latest.json");
  assert.equal(parseArgs(["--file", "/tmp/latest.json"]).file, "/tmp/latest.json");
  assert.equal(parseArgs(["--allow-subset"]).allowSubset, true);
});

test("parseArgs rejects conflicting sources and unknown options", () => {
  assert.throws(() => parseArgs(["--url", "https://a.test", "--file", "b"]), /either --url or --file/);
  assert.throws(() => parseArgs(["--bogus"]), /unknown option/);
  assert.throws(() => parseArgs(["--channel"]), /needs a value/);
  assert.throws(() => parseArgs(["--check-urls=yes"]), /does not take a value/);
  assert.throws(() => parseArgs(["positional"]), /unexpected argument/);
});

test("loadManifest reads a local file", async (t) => {
  const dir = mkdtempSync(join(tmpdir(), "verify-updater-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const path = join(dir, "latest.json");
  writeFileSync(path, JSON.stringify(fullManifest()));

  const manifest = await loadManifest({ url: null, file: path });
  assert.equal(manifest.version, "0.8.0");
  assert.equal(validateManifest(manifest).ok, true);
});

test("loadManifest surfaces invalid JSON", async (t) => {
  const dir = mkdtempSync(join(tmpdir(), "verify-updater-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const path = join(dir, "latest.json");
  writeFileSync(path, "{ not json");

  await assert.rejects(() => loadManifest({ url: null, file: path }));
});
