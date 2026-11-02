#!/usr/bin/env node
// scripts/verify-updater-manifest.mjs — validate a published Tauri updater
// manifest (`latest.json`).
//
// The manifest is the only file standing between a release and an update:
// a missing platform silently strands those users, a wrong URL 404s, and an
// empty signature fails verification after the download. This script checks
// all of it, before or after publishing.
//
// Usage:
//   node scripts/verify-updater-manifest.mjs
//   node scripts/verify-updater-manifest.mjs --channel beta
//   node scripts/verify-updater-manifest.mjs --url <https url> --check-urls
//   node scripts/verify-updater-manifest.mjs --file latest.json --allow-subset
//
// Exit codes: 0 = valid, 1 = invalid or unreachable.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { TARGETS, SEMVER } from "./updater-manifest.mjs";

// Re-exported so the verifier is the single test surface: callers do not need
// to know the manifest builder's module path to assert the platform allowlist.
export { TARGETS };

const REPO = "deuriib/agency-agents-app";
const TIMEOUT_MS = 20_000;

/** Channel → the exact URL the app fetches. Kept in one place so a rename
 *  cannot leave the verifier checking a different stream than the app. */
const CHANNEL_URLS = Object.freeze({
  stable: `https://github.com/${REPO}/releases/latest/download/latest.json`,
  beta: `https://github.com/${REPO}/releases/download/updater-beta/latest.json`,
});

const VALUE_FLAGS = new Set(["url", "file", "channel", "expect"]);
const BOOL_FLAGS = new Set(["check-urls", "allow-subset", "json", "help"]);

const USAGE = `verify-updater-manifest — validate a Tauri v2 updater manifest.

Usage
  node scripts/verify-updater-manifest.mjs [options]

Options
  --channel stable|beta   Stream to fetch (default: stable).
  --url <https url>       Check an explicit manifest URL instead of a channel.
  --file <path>           Check a local manifest instead of fetching one.
  --expect <a,b,c>        Platforms that must be present (default: all five).
  --allow-subset          Do not require any particular platform.
  --check-urls            HEAD every artifact URL and require a 2xx.
  --json                  Machine-readable report.
  -h, --help              Show this help.

Defaults
  stable → ${CHANNEL_URLS.stable}
  beta   → ${CHANNEL_URLS.beta}

The platform keys are Tauri's exact allowlist:
${TARGETS.join(", ")}
`;

export class VerifyError extends Error {}

export function targetUrl(channel) {
  const url = CHANNEL_URLS[channel];
  if (!url) {
    throw new VerifyError(
      `unknown channel "${channel}" (expected stable or beta)`,
    );
  }
  return url;
}

function isHttps(value) {
  try {
    return new URL(value).protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Validate the manifest shape. Pure: no I/O, no clock, no environment.
 *
 * `expected` is the list of platform keys that must be present. Pass `[]` to
 * accept whatever the manifest contains (still validated entry by entry).
 */
export function validateManifest(manifest, options = {}) {
  const expected = options.expected ?? TARGETS;
  const errors = [];
  const platforms = [];

  if (manifest === null || typeof manifest !== "object" || Array.isArray(manifest)) {
    return { ok: false, errors: ["manifest is not a JSON object"], platforms };
  }
  if (typeof manifest.version !== "string" || !SEMVER.test(manifest.version)) {
    errors.push(
      `version is not valid semver: ${JSON.stringify(manifest.version)}`,
    );
  }

  const entries = manifest.platforms;
  if (entries === null || typeof entries !== "object" || Array.isArray(entries)) {
    errors.push("platforms is not an object");
    return { ok: false, errors, platforms };
  }

  for (const [target, entry] of Object.entries(entries)) {
    if (!TARGETS.includes(target)) {
      errors.push(`unknown platform key "${target}" (Tauri ignores it silently)`);
      continue;
    }
    if (entry === null || typeof entry !== "object" || Array.isArray(entry)) {
      errors.push(`platforms["${target}"] is not an object`);
      continue;
    }
    const signature = typeof entry.signature === "string" ? entry.signature : "";
    const url = typeof entry.url === "string" ? entry.url : "";
    const entryOk = signature.trim() !== "" && isHttps(url);
    if (signature.trim() === "") {
      errors.push(`platforms["${target}"].signature is empty`);
    }
    if (!isHttps(url)) {
      errors.push(`platforms["${target}"].url is not an https URL`);
    }
    platforms.push({
      target,
      url,
      signatureBytes: signature.length,
      ok: entryOk,
    });
  }

  for (const target of expected) {
    if (!Object.prototype.hasOwnProperty.call(entries, target)) {
      errors.push(`missing expected platform "${target}"`);
    }
  }

  platforms.sort((a, b) => a.target.localeCompare(b.target));
  return { ok: errors.length === 0, errors, platforms };
}

export function parseArgs(argv) {
  const out = {
    channel: "stable",
    checkUrls: false,
    allowSubset: false,
    json: false,
    help: false,
    url: null,
    file: null,
    expect: null,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "-h" || arg === "--help") {
      out.help = true;
      continue;
    }
    if (!arg.startsWith("--")) {
      throw new VerifyError(`unexpected argument "${arg}"`);
    }
    const [key, inline] = arg.slice(2).split("=", 2);

    if (BOOL_FLAGS.has(key)) {
      if (inline !== undefined) {
        throw new VerifyError(`--${key} does not take a value`);
      }
      if (key === "check-urls") out.checkUrls = true;
      else if (key === "allow-subset") out.allowSubset = true;
      else if (key === "json") out.json = true;
      else out.help = true;
      continue;
    }

    if (!VALUE_FLAGS.has(key)) {
      throw new VerifyError(`unknown option "--${key}"`);
    }
    const value = inline ?? argv[++i];
    if (value === undefined || value === "") {
      throw new VerifyError(`--${key} needs a value`);
    }
    if (key === "url") out.url = value;
    else if (key === "file") out.file = value;
    else if (key === "channel") out.channel = value;
    else out.expect = value.split(",").map((part) => part.trim()).filter(Boolean);
  }

  if (out.url && out.file) {
    throw new VerifyError("use either --url or --file, not both");
  }
  return out;
}

export async function loadManifest({ url, file }) {
  if (file) {
    return JSON.parse(readFileSync(file, "utf8"));
  }
  const response = await fetch(url, {
    redirect: "follow",
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  if (!response.ok) {
    throw new VerifyError(`GET ${url} -> HTTP ${response.status}`);
  }
  return response.json();
}

/** HEAD the artifact; some object stores reject HEAD with 403/405, so fall
 *  back to a one-byte ranged GET without downloading the whole bundle. */
async function artifactStatus(url) {
  let response = await fetch(url, {
    method: "HEAD",
    redirect: "follow",
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  if (response.status === 403 || response.status === 405) {
    response = await fetch(url, {
      headers: { Range: "bytes=0-0" },
      redirect: "follow",
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
  }
  return response.status;
}

async function main(argv) {
  const options = parseArgs(argv);
  if (options.help) {
    console.log(USAGE);
    return 0;
  }

  const sourceFile = options.file;
  const sourceUrl = options.url ?? (sourceFile ? null : targetUrl(options.channel));
  const source = sourceFile ?? sourceUrl;

  let manifest;
  try {
    manifest = await loadManifest({ url: sourceUrl, file: sourceFile });
  } catch (error) {
    console.error(`✗ could not load ${source}: ${error.message}`);
    return 1;
  }

  const expected = options.allowSubset ? [] : (options.expect ?? TARGETS);
  const result = validateManifest(manifest, { expected });

  if (options.checkUrls) {
    for (const platform of result.platforms) {
      if (!platform.url) {
        platform.httpStatus = null;
        platform.ok = false;
        continue;
      }
      try {
        platform.httpStatus = await artifactStatus(platform.url);
        platform.ok =
          platform.ok &&
          platform.httpStatus >= 200 &&
          platform.httpStatus < 300;
      } catch (error) {
        platform.httpStatus = `error: ${error.message}`;
        platform.ok = false;
      }
    }
    if (!result.platforms.every((platform) => platform.ok)) {
      result.ok = false;
    }
  }

  if (options.json) {
    console.log(JSON.stringify({ source, ...result }, null, 2));
    return result.ok ? 0 : 1;
  }

  console.log(`Manifest: ${source}`);
  console.log(`Version:  ${manifest.version ?? "(missing)"}`);
  console.log("Platforms:");
  for (const platform of result.platforms) {
    const verdict = platform.ok ? "OK  " : "FAIL";
    const http =
      platform.httpStatus === undefined ? "" : ` http=${platform.httpStatus}`;
    console.log(
      `  ${verdict} ${platform.target.padEnd(18)} sig=${platform.signatureBytes}B${http}`,
    );
  }
  if (result.errors.length > 0) {
    console.log("Problems:");
    for (const error of result.errors) console.log(`  - ${error}`);
  }
  console.log(result.ok ? "Result: PASS" : "Result: FAIL");
  return result.ok ? 0 : 1;
}

const invokedDirectly =
  process.argv[1] !== undefined &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  main(process.argv.slice(2))
    .then((code) => {
      process.exitCode = code;
    })
    .catch((error) => {
      console.error(`✗ ${error.message}`);
      process.exitCode = 1;
    });
}
