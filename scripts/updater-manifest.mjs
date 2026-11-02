#!/usr/bin/env node
// scripts/updater-manifest.mjs — build or merge the Tauri v2 updater manifest
// (`latest.json`) that the in-app updater downloads.
//
// The manifest is the whole contract between a release and the updater:
//
//   {
//     "version": "0.8.0",
//     "notes": "…",
//     "pub_date": "2026-11-02T12:00:00Z",
//     "platforms": {
//       "darwin-aarch64": { "signature": "…", "url": "…" }
//     }
//   }
//
// A missing or malformed manifest makes every client report "up to date"
// forever (that is the 404 bug this script exists to end), so nothing is
// written unless every input validates. There are no npm dependencies: this
// runs on the release runner as plain Node 22 ESM.
//
// Exit codes: 0 = manifest written, 1 = nothing written (message on stderr).

import {
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Tauri v2 platform keys. Anything else is a typo that silently disables updates. */
export const TARGETS = Object.freeze([
  "darwin-aarch64",
  "darwin-x86_64",
  "windows-x86_64",
  "windows-aarch64",
  "linux-x86_64",
]);

const TARGET_SET = new Set(TARGETS);

// Official semver.org regex: rejects leading zeros and empty identifiers, so
// "1.02.3" and "1.0.0-" fail instead of producing a version Tauri cannot
// compare against installed versions.
export const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;

// Tauri parses pub_date as RFC3339; anything else is silently ignored upstream.
const RFC3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;

const VALUE_FLAGS = new Set([
  "version",
  "notes",
  "notes-file",
  "pub-date",
  "base-url",
  "assets-dir",
  "asset",
  "merge",
  "out",
]);

const FLAG_TO_KEY = {
  version: "version",
  notes: "notes",
  "notes-file": "notesFile",
  "pub-date": "pubDate",
  "base-url": "baseUrl",
  "assets-dir": "assetsDir",
  merge: "merge",
  out: "out",
};

const USAGE = `updater-manifest — build or merge a Tauri v2 updater manifest (latest.json).

Usage
  node scripts/updater-manifest.mjs --version <semver> --base-url <https dir url> \\
    [--notes <text> | --notes-file <path>] [--pub-date <rfc3339>] \\
    [--assets-dir <dir>] [--asset <target>=<filename>]... \\
    [--merge <existing.json>] \\
    --out <path|->

Options
  --version <semver>        Release version. A leading "v" is accepted and stripped
                            (v0.8.0 and 0.8.0 both produce "0.8.0").
  --notes <text>            Release notes shown by the updater. Omit for none.
  --notes-file <path>       Read the notes from a file instead of --notes.
  --pub-date <rfc3339>      Publication time, e.g. 2026-11-02T12:00:00Z.
                            Omit and the key is left out of the manifest.
  --base-url <https url>    Public directory the artifacts live in. The URL for
                            each artifact becomes <base-url>/<encoded filename>.
                            Must be https; a query or fragment is rejected.
  --assets-dir <dir>        Directory holding the artifacts and their .sig files.
  --asset <target>=<file>   Repeatable, one per platform. The signature is read
                            verbatim from <file>.sig next to the artifact.
  --merge <existing.json>   Keep platforms from an existing manifest that this
                            run does not override. A path that does not exist
                            yet means "nothing to merge" (first release of a
                            channel), not an error.
  --out <path|->            Where to write the manifest. "-" writes it to stdout.
  -h, --help                Show this help.

Targets (exact Tauri v2 keys)
  ${TARGETS.join(", ")}

Behaviour
  * Platforms are sorted by target key, so re-running on the same inputs
    produces byte-identical output.
  * notes and pub_date are omitted when not supplied; version is always present.
  * Every artifact and every signature is checked before anything is written:
    no partial manifest, ever. A failure exits 1 with what went wrong and the
    exact next step.
  * Merged entries are re-validated (https URL, non-empty signature) so a stale
    or hand-edited manifest cannot poison a release.

Examples
  # macOS artifacts published as release assets
  node scripts/updater-manifest.mjs \\
    --version 0.8.0 \\
    --notes "Fixes the updater manifest." \\
    --pub-date 2026-11-02T12:00:00Z \\
    --base-url https://github.com/deuriib/agency-agents-app/releases/download/v0.8.0 \\
    --assets-dir dist \\
    --asset darwin-aarch64=Agency.Agents_0.8.0_aarch64.app.tar.gz \\
    --asset darwin-x86_64=Agency.Agents_0.8.0_x64.app.tar.gz \\
    --out dist/latest.json

  # later platform run, keeping the entries already published
  node scripts/updater-manifest.mjs ... \\
    --asset windows-x86_64=Agency.Agents_0.8.0_x64-setup.exe \\
    --merge dist/latest.json \\
    --out dist/latest.json

Exit codes
  0  manifest written
  1  validation or I/O failure — nothing was written`;

/** Error type for anything the user can fix from the message. */
export class ManifestError extends Error {
  constructor(message) {
    super(message);
    this.name = "ManifestError";
  }
}

/**
 * Parse argv into an options object for `buildManifest`.
 * Throws ManifestError on unknown flags, missing values and duplicate flags.
 * Pure: reads no files and no environment.
 *
 * @param {readonly string[]} argv
 * @returns {{
 *   help: boolean,
 *   version?: string,
 *   notes?: string,
 *   notesFile?: string,
 *   pubDate?: string,
 *   baseUrl?: string,
 *   assetsDir?: string,
 *   assets: Array<{ target: string, filename: string }>,
 *   merge?: string,
 *   out?: string,
 * }}
 */
export function parseArgs(argv) {
  const args = {
    help: false,
    version: undefined,
    notes: undefined,
    notesFile: undefined,
    pubDate: undefined,
    baseUrl: undefined,
    assetsDir: undefined,
    assets: [],
    merge: undefined,
    out: undefined,
  };
  const seen = new Set();

  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];

    if (token === "--help" || token === "-h") {
      args.help = true;
      continue;
    }
    if (typeof token !== "string" || !token.startsWith("--")) {
      throw new ManifestError(
        `Unexpected argument "${token}". Options start with --; run with --help to see them.`,
      );
    }

    const equals = token.indexOf("=");
    const flag = equals === -1 ? token.slice(2) : token.slice(2, equals);
    let value = equals === -1 ? undefined : token.slice(equals + 1);

    if (!VALUE_FLAGS.has(flag)) {
      throw new ManifestError(
        `Unknown option "--${flag}". Run with --help to see the supported flags.`,
      );
    }

    if (value === undefined) {
      value = argv[i + 1];
      if (value === undefined || (typeof value === "string" && value.startsWith("--"))) {
        throw new ManifestError(
          `Option "--${flag}" requires a value. If the value itself starts with "--", use --${flag}=<value>.`,
        );
      }
      i += 1;
    }

    if (flag === "asset") {
      const split = value.indexOf("=");
      if (split <= 0 || split === value.length - 1) {
        throw new ManifestError(
          `--asset must be <target>=<filename>, got "${value}" — e.g. --asset darwin-aarch64=Agency.Agents_0.8.0_aarch64.app.tar.gz.`,
        );
      }
      args.assets.push({ target: value.slice(0, split), filename: value.slice(split + 1) });
      continue;
    }

    if (seen.has(flag)) {
      throw new ManifestError(`Duplicate option "--${flag}". Pass it once.`);
    }
    seen.add(flag);
    args[FLAG_TO_KEY[flag]] = value;
  }

  return args;
}

/**
 * Build the manifest object. Synchronous and deterministic: it reads the
 * artifacts' signatures and the merge file from disk, but never the clock or
 * the environment.
 *
 * @param {{
 *   version?: string,
 *   notes?: string,
 *   notesFile?: string,
 *   pubDate?: string,
 *   baseUrl?: string,
 *   assetsDir?: string,
 *   assets?: Array<{ target: string, filename: string }>,
 *   merge?: string,
 * }} [options]
 * @returns {{ version: string, notes?: string, pub_date?: string, platforms: Record<string, { signature: string, url: string }> }}
 */
export function buildManifest(options = {}) {
  const version = normaliseVersion(options.version);
  const baseUrl = normaliseBaseUrl(options.baseUrl);
  const notes = resolveNotes(options);
  const pubDate = normalisePubDate(options.pubDate);
  const assets = Array.isArray(options.assets) ? options.assets : [];

  const platforms = {};

  // Merge first so an asset always wins over a stale entry for the same target.
  const merged = readMergedPlatforms(options.merge);
  for (const [target, entry] of Object.entries(merged)) {
    platforms[target] = validateMergedEntry(target, entry, options.merge);
  }

  if (assets.length > 0) {
    if (options.assetsDir === undefined || options.assetsDir === null || String(options.assetsDir) === "") {
      throw new ManifestError(
        "--assets-dir is required when --asset is used — e.g. --assets-dir dist.",
      );
    }
    const assetsDir = resolve(String(options.assetsDir));
    const seenTargets = new Set();
    for (const asset of assets) {
      const target = String(asset.target);
      if (seenTargets.has(target)) {
        throw new ManifestError(
          `Duplicate --asset for target "${target}". Pass each target once; the last one would silently win.`,
        );
      }
      seenTargets.add(target);
      platforms[target] = readAssetEntry(target, asset.filename, assetsDir, baseUrl);
    }
  }

  if (Object.keys(platforms).length === 0) {
    throw new ManifestError(
      "No platforms to write — pass at least one --asset (or --merge a manifest that already has platforms).",
    );
  }

  const manifest = { version };
  if (notes !== undefined) manifest.notes = notes;
  if (pubDate !== undefined) manifest.pub_date = pubDate;
  manifest.platforms = sortPlatforms(platforms);
  return manifest;
}

// ---------------------------------------------------------------------------
// validation helpers
// ---------------------------------------------------------------------------

function normaliseVersion(raw) {
  if (typeof raw !== "string" || raw.trim() === "") {
    throw new ManifestError("--version is required — e.g. --version 1.4.0.");
  }
  const value = raw.trim().replace(/^[vV]/, "");
  if (!SEMVER.test(value)) {
    throw new ManifestError(
      `"--version ${raw.trim()}" is not a semver — use MAJOR.MINOR.PATCH with an optional -prerelease, e.g. 1.4.0 or 1.4.0-beta.1.`,
    );
  }
  return value;
}

function normaliseBaseUrl(raw) {
  if (typeof raw !== "string" || raw.trim() === "") {
    throw new ManifestError(
      "--base-url is required — the public directory the artifacts are uploaded to, e.g. https://github.com/owner/repo/releases/download/v1.4.0.",
    );
  }
  const value = raw.trim().replace(/\/+$/, "");
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new ManifestError(
      `--base-url "${raw.trim()}" is not a valid URL — e.g. https://github.com/owner/repo/releases/download/v1.4.0.`,
    );
  }
  if (url.protocol !== "https:") {
    throw new ManifestError(
      `--base-url must be https (got "${url.protocol}//…") — Tauri refuses plain-http update endpoints.`,
    );
  }
  if (url.search !== "" || url.hash !== "") {
    throw new ManifestError(
      `--base-url must be a directory URL without a query or fragment (got "${raw.trim()}").`,
    );
  }
  return value;
}

function resolveNotes(options) {
  const hasInline = options.notes !== undefined && options.notes !== null;
  const hasFile = options.notesFile !== undefined && options.notesFile !== null;
  if (hasInline && hasFile) {
    throw new ManifestError(
      "--notes and --notes-file are mutually exclusive — pass one of them.",
    );
  }
  if (!hasInline && !hasFile) return undefined;

  let text;
  if (hasFile) {
    const notesPath = resolve(String(options.notesFile));
    if (!existsSync(notesPath) || !statSync(notesPath).isFile()) {
      throw new ManifestError(`--notes-file not found: ${notesPath} — check the path.`);
    }
    text = readFileSync(notesPath, "utf8");
  } else {
    text = String(options.notes);
  }

  // Only the trailing line break is dropped; the body stays verbatim.
  text = text.replace(/[\r\n]+$/, "");
  return text === "" ? undefined : text;
}

function normalisePubDate(raw) {
  if (raw === undefined || raw === null) return undefined;
  const value = String(raw).trim();
  if (value === "") return undefined;
  if (!RFC3339.test(value) || Number.isNaN(Date.parse(value))) {
    throw new ManifestError(
      `--pub-date "${value}" is not an RFC3339 timestamp — e.g. $(date -u +%Y-%m-%dT%H:%M:%SZ) produces 2026-11-02T12:00:00Z.`,
    );
  }
  return value;
}

function readMergedPlatforms(mergePath) {
  if (mergePath === undefined || mergePath === null || String(mergePath) === "") return {};

  const path = resolve(String(mergePath));
  // Absent file = first release on this channel. Not an error; the release
  // workflow passes --merge unconditionally.
  if (!existsSync(path)) return {};

  let parsed;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch (err) {
    throw new ManifestError(
      `Cannot merge ${path}: invalid JSON (${err.message}) — fix the file or drop --merge.`,
    );
  }
  if (!isPlainObject(parsed)) {
    throw new ManifestError(
      `Cannot merge ${path}: expected a JSON object at the top level — fix the file or drop --merge.`,
    );
  }
  const platforms = parsed.platforms === undefined ? {} : parsed.platforms;
  if (!isPlainObject(platforms)) {
    throw new ManifestError(
      `Cannot merge ${path}: "platforms" must be an object — fix the file or drop --merge.`,
    );
  }
  return platforms;
}

function validateMergedEntry(target, entry, mergePath) {
  if (!TARGET_SET.has(target)) {
    throw new ManifestError(
      `Cannot merge ${mergePath}: unknown platform "${target}". Allowed targets: ${TARGETS.join(", ")}.`,
    );
  }
  if (!isPlainObject(entry)) {
    throw new ManifestError(
      `Cannot merge ${mergePath}: platform "${target}" must be an object with signature and url — fix the file or drop --merge.`,
    );
  }
  const { signature, url } = entry;
  if (typeof signature !== "string" || signature.trim() === "") {
    throw new ManifestError(
      `Cannot merge ${mergePath}: platform "${target}" has no signature — re-run the release for it or drop --merge.`,
    );
  }
  if (typeof url !== "string" || url.trim() === "") {
    throw new ManifestError(
      `Cannot merge ${mergePath}: platform "${target}" has no url — re-run the release for it or drop --merge.`,
    );
  }
  let parsedUrl;
  try {
    parsedUrl = new URL(url);
  } catch {
    throw new ManifestError(
      `Cannot merge ${mergePath}: platform "${target}" url "${url}" is not a valid URL — fix the file or drop --merge.`,
    );
  }
  if (parsedUrl.protocol !== "https:") {
    throw new ManifestError(
      `Cannot merge ${mergePath}: platform "${target}" url must be https (got "${url}") — fix the file or drop --merge.`,
    );
  }
  return { signature, url };
}

function readAssetEntry(target, filename, assetsDir, baseUrl) {
  if (!TARGET_SET.has(target)) {
    throw new ManifestError(
      `Unknown target "${target}". Allowed targets: ${TARGETS.join(", ")}.`,
    );
  }
  if (typeof filename !== "string" || filename === "") {
    throw new ManifestError(`--asset ${target}= has an empty filename.`);
  }
  if (
    isAbsolute(filename) ||
    filename.includes("/") ||
    filename.includes("\\") ||
    filename === "." ||
    filename === ".."
  ) {
    throw new ManifestError(
      `--asset ${target}=${filename}: filename must be a bare file name inside --assets-dir, not a path.`,
    );
  }

  const artifact = resolve(assetsDir, filename);
  if (!existsSync(artifact) || !statSync(artifact).isFile()) {
    throw new ManifestError(
      `Artifact not found: ${artifact} — check --assets-dir and the --asset ${target}=${filename} filename.`,
    );
  }

  const signaturePath = `${artifact}.sig`;
  if (!existsSync(signaturePath) || !statSync(signaturePath).isFile()) {
    throw new ManifestError(
      `Signature not found: ${signaturePath} — the release must upload the Tauri signature next to the artifact (tauri signer sign writes <artifact>.sig).`,
    );
  }
  // Verbatim signature: strip only the trailing line break the signer adds.
  const signature = readFileSync(signaturePath, "utf8").replace(/[\r\n]+$/, "");
  if (signature.trim() === "") {
    throw new ManifestError(
      `Signature is empty: ${signaturePath} — re-run \`tauri signer sign\` for this artifact.`,
    );
  }

  return { signature, url: `${baseUrl}/${encodeURIComponent(filename)}` };
}

function sortPlatforms(platforms) {
  const sorted = {};
  // Default sort is code-unit order, not locale order: deterministic on every runner.
  for (const target of Object.keys(platforms).sort()) {
    sorted[target] = platforms[target];
  }
  return sorted;
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

function writeAtomic(path, text) {
  const directory = dirname(path);
  mkdirSync(directory, { recursive: true });
  const temporary = `${path}.tmp`;
  try {
    writeFileSync(temporary, text);
    renameSync(temporary, path);
  } catch (err) {
    rmSync(temporary, { force: true });
    throw new ManifestError(`Cannot write ${path}: ${err.message} — check the directory permissions.`);
  }
}

function run(argv) {
  if (argv.length === 0) {
    process.stderr.write(`${USAGE}\n\nNothing to do: no options were given. Start with --help.\n`);
    return 1;
  }
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write(`${USAGE}\n`);
    return 0;
  }

  const args = parseArgs(argv);
  if (args.out === undefined || args.out === "") {
    throw new ManifestError(
      "--out is required — e.g. --out dist/latest.json (or --out - to write to stdout).",
    );
  }

  const manifest = buildManifest(args);
  const text = `${JSON.stringify(manifest, null, 2)}\n`;
  const targets = Object.keys(manifest.platforms);

  if (args.out === "-") {
    process.stdout.write(text);
    return 0;
  }

  const outPath = resolve(args.out);
  writeAtomic(outPath, text);
  const plural = targets.length === 1 ? "platform" : "platforms";
  process.stderr.write(
    `updater-manifest: wrote ${outPath} (${targets.length} ${plural}: ${targets.join(", ")})\n`,
  );
  return 0;
}

function invokedDirectly() {
  const entry = process.argv[1];
  if (!entry) return false;
  try {
    return realpathSync(entry) === realpathSync(fileURLToPath(import.meta.url));
  } catch {
    return false;
  }
}

if (invokedDirectly()) {
  try {
    process.exitCode = run(process.argv.slice(2));
  } catch (err) {
    if (err instanceof ManifestError) {
      process.stderr.write(`updater-manifest: ${err.message}\n`);
    } else {
      process.stderr.write(`updater-manifest: unexpected failure: ${err?.message ?? err}\n`);
      if (process.env.UPDATER_MANIFEST_DEBUG === "1" && err?.stack) {
        process.stderr.write(`${err.stack}\n`);
      }
    }
    process.exitCode = 1;
  }
}
