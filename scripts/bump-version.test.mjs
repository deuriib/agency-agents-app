import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const SCRIPT = new URL("../scripts/bump-version.mjs", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");

test("rejects missing version", () => {
  const dir = mkdtempSync(join(tmpdir(), "bv-"));
  try {
    assert.throws(() => execFileSync("node", [SCRIPT], { cwd: dir, stdio: "pipe" }));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("rejects non-semver", () => {
  const dir = mkdtempSync(join(tmpdir(), "bv-"));
  try {
    assert.throws(() => execFileSync("node", [SCRIPT, "not-a-version"], { cwd: dir, stdio: "pipe" }));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("bumps all manifests", async () => {
  const dir = mkdtempSync(join(tmpdir(), "bv-"));
  try {
    execFileSync("git", ["init", "-q"], { cwd: dir });
    execFileSync("git", ["config", "user.email", "t@t.com"], { cwd: dir });
    execFileSync("git", ["config", "user.name", "t"], { cwd: dir });

    const { mkdirSync } = await import("node:fs");
    mkdirSync(join(dir, "src-tauri"), { recursive: true });
    mkdirSync(join(dir, "scripts"), { recursive: true });

    writeFileSync(join(dir, "package.json"), JSON.stringify({ name: "agency-agents-app", version: "0.3.2" }, null, 2) + "\n");
    writeFileSync(join(dir, "package-lock.json"), JSON.stringify({ name: "agency-agents-app", version: "0.3.2", packages: { "": { name: "agency-agents-app", version: "0.3.2" } } }, null, 2) + "\n");
    writeFileSync(join(dir, "src-tauri/tauri.conf.json"), JSON.stringify({ version: "0.3.2" }, null, 2) + "\n");
    writeFileSync(join(dir, "src-tauri/Cargo.toml"), `[package]\nname = "agency-agents-app"\nversion = "0.3.2"\nedition = "2021"\n`);
    writeFileSync(join(dir, "src-tauri/Cargo.lock"), `[[package]]\nname = "agency-agents-app"\nversion = "0.3.2"\n\n[[package]]\nname = "other"\nversion = "1.0.0"\n`);

    const scriptSrc = readFileSync(SCRIPT, "utf8");
    writeFileSync(join(dir, "scripts/bump-version.mjs"), scriptSrc);

    execFileSync("git", ["add", "-A"], { cwd: dir });
    execFileSync("git", ["commit", "-q", "-m", "init"], { cwd: dir });

    execFileSync("node", [join(dir, "scripts/bump-version.mjs"), "0.4.0"], { cwd: dir, stdio: "pipe" });

    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    assert.equal(pkg.version, "0.4.0");

    const lockJson = JSON.parse(readFileSync(join(dir, "package-lock.json"), "utf8"));
    assert.equal(lockJson.version, "0.4.0");
    assert.equal(lockJson.packages[""].version, "0.4.0");

    const tauri = JSON.parse(readFileSync(join(dir, "src-tauri/tauri.conf.json"), "utf8"));
    assert.equal(tauri.version, "0.4.0");

    const cargo = readFileSync(join(dir, "src-tauri/Cargo.toml"), "utf8");
    assert.match(cargo, /version = "0.4.0"/);

    const lock = readFileSync(join(dir, "src-tauri/Cargo.lock"), "utf8");
    assert.match(lock, /name = "agency-agents-app"\r?\nversion = "0.4.0"/);
    assert.match(lock, /name = "other"\nversion = "1.0.0"/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});