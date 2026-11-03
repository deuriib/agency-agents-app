import assert from "node:assert/strict";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, describe, it } from "node:test";

import {
  AGENTS_ROOT,
  CORPUS_BASELINE,
  OVERLAY_PATH,
  RULES_BASELINE,
  RULES_ROOT,
  RULE_CONTRACT_PATH,
  TOOLS_PATH,
  applyOverlay,
  check,
  contracts,
  diffBaseline,
  corpusBaselineFiles,
  rulesBaselineFiles,
  sync,
  writeBaseline,
} from "./sync-vendor.mjs";

const ROOT = new URL("..", import.meta.url).pathname;
const CATALOG = {
  tools: {
    "alpha-tool": {
      id: "alphaTool",
      label: "Alpha",
      kebab: "alpha-tool",
      detect: { dirs: [".alpha/agents"], agentsDir: ".alpha/agents" },
    },
    "beta-tool": {
      id: "betaTool",
      label: "Beta",
      kebab: "beta-tool",
      detect: { dirs: [], agentsDir: ".beta" },
    },
  },
};

describe("applyOverlay", () => {
  it("replaces detect.dirs and keeps the rest of the tool intact", () => {
    const { tools, applied } = applyOverlay(CATALOG, {
      overrides: { "alpha-tool": { detectDirs: [".alpha"], _why: "detect the CLI home" } },
    });
    assert.deepEqual(tools["alpha-tool"].detect.dirs, [".alpha"]);
    assert.equal(tools["alpha-tool"].detect.agentsDir, ".alpha/agents");
    assert.equal(tools["alpha-tool"].label, "Alpha");
    assert.deepEqual(applied, ["alpha-tool.detectDirs"]);
  });

  it("does not mutate the catalog it was given", () => {
    applyOverlay(CATALOG, { overrides: { "alpha-tool": { detectDirs: [".alpha"] } } });
    assert.deepEqual(CATALOG.tools["alpha-tool"].detect.dirs, [".alpha/agents"]);
  });

  it("rejects an override that duplicates upstream", () => {
    assert.throws(
      () => applyOverlay(CATALOG, { overrides: { "alpha-tool": { detectDirs: [".alpha/agents"] } } }),
      /duplicates upstream/,
    );
  });

  it("rejects an unknown tool", () => {
    assert.throws(
      () => applyOverlay(CATALOG, { overrides: { "no-such-tool": { detectDirs: [".x"] } } }),
      /does not declare/,
    );
  });

  it("rejects a field outside the closed set", () => {
    assert.throws(
      () => applyOverlay(CATALOG, { overrides: { "alpha-tool": { dest: {} } } }),
      /not an overridable field/,
    );
  });
});

describe("contracts", () => {
  it("copies every non-overridden tool field from the pinned catalog", () => {
    const generated = JSON.parse(contracts()[TOOLS_PATH]);
    const vendor = JSON.parse(readFileSync(join(ROOT, AGENTS_ROOT, "tools.json"), "utf8"));
    const overlay = JSON.parse(readFileSync(join(ROOT, OVERLAY_PATH), "utf8"));

    assert.deepEqual(Object.keys(generated.tools).sort(), Object.keys(vendor.tools).sort());
    for (const [kebab, tool] of Object.entries(vendor.tools)) {
      const overridden = overlay.overrides[kebab] ?? {};
      for (const [field, value] of Object.entries(tool)) {
        if (field === "detect" && "detectDirs" in overridden) continue;
        assert.deepEqual(generated.tools[kebab][field], value, `${kebab}.${field} must match upstream`);
      }
    }
  });

  it("names every applied override in the generated note", () => {
    const generated = JSON.parse(contracts()[TOOLS_PATH]);
    assert.match(generated._note, /tools-overlay\.json/);
    assert.match(generated._note, /GENERATED/);
  });

  it("re-emits the rule contract verbatim, under a generated note", () => {
    const generated = JSON.parse(contracts()[RULE_CONTRACT_PATH]);
    const vendor = JSON.parse(readFileSync(join(ROOT, RULES_ROOT, "tools.json"), "utf8"));
    assert.deepEqual(generated.harnesses, vendor.harnesses);
    assert.equal(generated.version, vendor.version);
  });
});

describe("baselines", () => {
  it("covers every division markdown file and the repo-level content", () => {
    const files = corpusBaselineFiles();
    assert.ok(files.has("divisions.json"), "divisions.json travels with the baseline");
    assert.ok(
      [...files.keys()].some((path) => path.startsWith("strategy/")),
      "strategy/ unlocks runbooks offline",
    );
    assert.ok([...files.keys()].some((path) => path.endsWith(".md") && path.includes("/")));
  });

  it("mirrors the rules corpus", () => {
    const files = rulesBaselineFiles();
    assert.ok([...files.keys()].every((path) => path.startsWith("rules/")));
    assert.ok(files.has("rules/_schema.json"));
  });

  it("reports missing, stale and extra files", () => {
    const dir = mkdtempSync(join(tmpdir(), "sync-vendor-"));
    after(() => rmSync(dir, { recursive: true, force: true }));
    const source = join(dir, "src");
    writeFileSync(join(dir, "one.md"), "one");
    writeFileSync(join(dir, "two.md"), "two");
    const files = new Map([
      ["one.md", join(dir, "one.md")],
      ["nested/two.md", join(dir, "two.md")],
    ]);

    assert.deepEqual(diffBaseline(source, files).missing.sort(), ["nested/two.md", "one.md"]);

    writeBaseline(source, files);
    assert.deepEqual(diffBaseline(source, files), { missing: [], changed: [], extra: [] });

    writeFileSync(join(source, "one.md"), "changed");
    writeFileSync(join(source, "stray.md"), "stray");
    const diff = diffBaseline(source, files);
    assert.deepEqual(diff.changed, ["one.md"]);
    assert.deepEqual(diff.extra, ["stray.md"]);

    // A second write prunes the stray file and refreshes the stale one.
    writeBaseline(source, files);
    assert.deepEqual(diffBaseline(source, files), { missing: [], changed: [], extra: [] });
    assert.equal(readFileSync(join(source, "one.md"), "utf8"), "one");
  });
});

describe("the checked-in tree", () => {
  it("matches the pinned submodules", () => {
    const { problems } = check();
    assert.deepEqual(problems, [], "run `mise run sync-vendor`");
  });

  it("is byte-identical after a regeneration", () => {
    const before = new Map(
      [TOOLS_PATH, RULE_CONTRACT_PATH, `${CORPUS_BASELINE}/divisions.json`].map((path) => [
        path,
        readFileSync(join(ROOT, path), "utf8"),
      ]),
    );
    const scratch = mkdtempSync(join(tmpdir(), "sync-vendor-root-"));
    after(() => rmSync(scratch, { recursive: true, force: true }));
    for (const dir of ["vendor", "src-tauri/data", "src-tauri/resources"]) {
      cpSync(join(ROOT, dir), join(scratch, dir), { recursive: true });
    }
    sync(scratch);
    for (const [path, content] of before) {
      assert.equal(readFileSync(join(scratch, path), "utf8"), content, `${path} must be reproducible`);
    }
  });
});
