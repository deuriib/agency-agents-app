#!/usr/bin/env node
// scripts/bump-version.mjs — bump the four version manifests in lockstep.
//
// Usage: node scripts/bump-version.mjs 0.4.0
//
// Manifests touched (in this order):
//   1. package.json
//   2. src-tauri/tauri.conf.json
//   3. src-tauri/Cargo.toml
//   4. src-tauri/Cargo.lock  (the `agency-agents-app` package entry only)
//
// The script is idempotent: re-running with the same version is a no-op
// on the manifests (it still prints the diff). It refuses a non-semver
// version and refuses to run when the working tree is dirty, so a bump
// always lands as a single, reviewable commit.

import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.url.replace("file:///", "").replace(/\/scripts\/.*$/, ""));

const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/;

function die(msg) {
  console.error(`bump-version: ${msg}`);
  process.exit(1);
}

function readJSON(p) {
  return JSON.parse(readFileSync(p, "utf8"));
}

function writeJSON(p, obj) {
  writeFileSync(p, JSON.stringify(obj, null, 2) + "\n");
}

const version = process.argv[2];
if (!version) die("missing version argument (e.g. 0.4.0)");
if (!SEMVER.test(version)) die(`not a semver: ${version}`);

// Refuse to run on a dirty tree so the bump is always a clean, single commit.
try {
  const status = execFileSync("git", ["status", "--porcelain"], { cwd: ROOT, encoding: "utf8" });
  if (status.trim() !== "") {
    die("working tree is dirty; commit or stash first");
  }
} catch {
  // Not a git checkout — allow (e.g. extracted tarball).
}

const files = {
  package: resolve(ROOT, "package.json"),
  tauri: resolve(ROOT, "src-tauri/tauri.conf.json"),
  cargo: resolve(ROOT, "src-tauri/Cargo.toml"),
  lock: resolve(ROOT, "src-tauri/Cargo.lock"),
};

// 1. package.json — targeted replacement to preserve formatting.
const pkgText = readFileSync(files.package, "utf8");
const pkgRe = /("version"\s*:\s*")[^"]*(")/;
if (!pkgRe.test(pkgText)) die("could not find version in package.json");
const newPkg = pkgText.replace(pkgRe, `$1${version}$2`);
writeFileSync(files.package, newPkg);

// 2. tauri.conf.json — targeted replacement to preserve formatting.
const tauriText = readFileSync(files.tauri, "utf8");
const tauriRe = /("version"\s*:\s*")[^"]*(")/;
if (!tauriRe.test(tauriText)) die("could not find version in tauri.conf.json");
const newTauri = tauriText.replace(tauriRe, `$1${version}$2`);
writeFileSync(files.tauri, newTauri);

// 3. Cargo.toml — only the [package] version line.
const cargoText = readFileSync(files.cargo, "utf8");
const cargoRe = /^version\s*=\s*"[^"]*"\s*$/m;
if (!cargoRe.test(cargoText)) die("could not find [package] version in Cargo.toml");
const prevCargo = (cargoText.match(cargoRe) || [])[0];
const newCargo = cargoText.replace(cargoRe, `version = "${version}"`);
writeFileSync(files.cargo, newCargo);

// 4. Cargo.lock — update the agency-agents-app package entry only.
const lockText = readFileSync(files.lock, "utf8");
const lockRe = /(name = "agency-agents-app"\r?\nversion = ")[^"]*(")/;
if (!lockRe.test(lockText)) die("could not find agency-agents-app entry in Cargo.lock");
const newLock = lockText.replace(lockRe, `$1${version}$2`);
writeFileSync(files.lock, newLock);

console.log(`bumped to ${version}`);
console.log("  package.json");
console.log("  src-tauri/tauri.conf.json");
console.log("  src-tauri/Cargo.toml");
console.log("  src-tauri/Cargo.lock");
console.log("");
console.log("Next: open a PR with these changes; release.yml creates the tag + draft release once merged.");