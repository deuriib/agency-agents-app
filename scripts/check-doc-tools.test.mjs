import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, cpSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  badgeLabels,
  installableTools,
  parseRenderFormats,
  readmeRows,
  scopeLabel,
  singleSourceWiring,
} from "./check-doc-tools.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const SCRIPT = join(HERE, "check-doc-tools.mjs");

/** A throwaway copy of every input the check reads, safe to mutate. */
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), "doc-tools-"));
  mkdirSync(join(dir, "src-tauri/data"), { recursive: true });
  mkdirSync(join(dir, "src-tauri/src"), { recursive: true });
  mkdirSync(join(dir, "src/lib/data"), { recursive: true });
  mkdirSync(join(dir, "landing"), { recursive: true });
  cpSync(join(ROOT, "src-tauri/data/tools.json"), join(dir, "src-tauri/data/tools.json"));
  cpSync(join(ROOT, "src-tauri/data/render-formats.json"), join(dir, "src-tauri/data/render-formats.json"));
  cpSync(join(ROOT, "src-tauri/src/registry.rs"), join(dir, "src-tauri/src/registry.rs"));
  cpSync(join(ROOT, "src/lib/data/toolRegistry.ts"), join(dir, "src/lib/data/toolRegistry.ts"));
  cpSync(join(ROOT, "landing/index.html"), join(dir, "landing/index.html"));
  cpSync(join(ROOT, "README.md"), join(dir, "README.md"));
  return dir;
}

/** Run the checker against `dir` and require a clean failure. */
function expectFailure(dir, pattern) {
  assert.throws(
    () => execFileSync(process.execPath, [SCRIPT, dir], { stdio: "pipe" }),
    (error) => {
      assert.equal(error.status, 1);
      assert.match(String(error.stderr), pattern);
      return true;
    },
  );
}

test("the repository docs match the registry", () => {
  const out = execFileSync(process.execPath, [SCRIPT], { cwd: ROOT, encoding: "utf8" });
  assert.match(out, /badges and 13 README rows match the registry/);
});

test("fails with the tool named when a landing badge is dropped", () => {
  const dir = fixture();
  try {
    const file = join(dir, "landing/index.html");
    const html = readFileSync(file, "utf8");
    const dropped = html.replace(/\s*<div class="badge"><code>Antigravity<\/code><\/div>/, "");
    assert.notEqual(dropped, html, "fixture no longer contains the Antigravity badge");
    writeFileSync(file, dropped);
    expectFailure(dir, /landing badge missing: Antigravity/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("fails when a recognized-only tool is still badged as installable", () => {
  const dir = fixture();
  try {
    const file = join(dir, "landing/index.html");
    const html = readFileSync(file, "utf8");
    const injected = html.replace(
      '<div class="badge"><code>Oh My Pi</code></div>',
      '<div class="badge"><code>Oh My Pi</code></div>\n        <div class="badge"><code>Windsurf</code></div>',
    );
    assert.notEqual(injected, html, "fixture no longer contains the Oh My Pi badge");
    writeFileSync(file, injected);
    expectFailure(dir, /landing badge not installable: Windsurf/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("fails when the README table is missing a tool", () => {
  const dir = fixture();
  try {
    const file = join(dir, "README.md");
    const markdown = readFileSync(file, "utf8");
    const stripped = markdown.replace(/^\| ZCode \|.*\n/m, "");
    assert.notEqual(stripped, markdown, "fixture no longer has the ZCode row");
    writeFileSync(file, stripped);
    expectFailure(dir, /README row missing: ZCode/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("fails when the README scope column contradicts tools.json", () => {
  const dir = fixture();
  try {
    const file = join(dir, "README.md");
    const markdown = readFileSync(file, "utf8");
    const wrong = markdown.replace(/^\| Claude Code \| user \+ project \|/m, "| Claude Code | user |");
    assert.notEqual(wrong, markdown, "fixture no longer has the Claude Code row in the expected shape");
    writeFileSync(file, wrong);
    expectFailure(dir, /README scope wrong: Claude Code: says "user", registry says "user \+ project"/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("render-formats.json is the single shared source", () => {
  const spec = JSON.parse(readFileSync(join(ROOT, "src-tauri/data/render-formats.json"), "utf8"));
  const formats = parseRenderFormats(spec);
  assert.ok(formats.has("identity"), "identity is a rendered format");
  assert.ok(formats.has("antigravity-md"), "antigravity-md is a rendered format");
  assert.ok(!formats.has("aider-conventions"), "aider has no renderer yet");
  assert.deepEqual(singleSourceWiring(ROOT), { rust: true, frontend: true });
});

test("parseRenderFormats rejects an empty or duplicated list", () => {
  assert.throws(() => parseRenderFormats({ formats: [] }), /non-empty array/);
  assert.throws(() => parseRenderFormats({}), /non-empty array/);
  assert.throws(() => parseRenderFormats({ formats: ["identity", "identity"] }), /duplicate/);
});

test("fails when the Rust side stops reading the shared file", () => {
  const dir = fixture();
  try {
    const file = join(dir, "src-tauri/src/registry.rs");
    const source = readFileSync(file, "utf8");
    const forked = source.replace(
      'const RENDER_FORMATS_JSON: &str = include_str!("../data/render-formats.json");',
      'const RENDER_FORMATS_JSON: &str = "{\\"formats\\":[\\"identity\\"]}";',
    );
    assert.notEqual(forked, source, "fixture no longer embeds render-formats.json the expected way");
    writeFileSync(file, forked);
    expectFailure(dir, /registry\.rs no longer embeds data\/render-formats\.json/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("fails when the frontend stops reading the shared file", () => {
  const dir = fixture();
  try {
    const file = join(dir, "src/lib/data/toolRegistry.ts");
    const source = readFileSync(file, "utf8");
    const forked = source.replace(
      'import spec from "../../../src-tauri/data/render-formats.json";',
      "const spec = { formats: [\"identity\"] };",
    );
    assert.notEqual(forked, source, "fixture no longer imports render-formats.json");
    writeFileSync(file, forked);
    expectFailure(dir, /toolRegistry\.ts no longer imports data\/render-formats\.json/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("adding a format to the shared file forces the docs to catch up", () => {
  const dir = fixture();
  try {
    const file = join(dir, "src-tauri/data/render-formats.json");
    const spec = JSON.parse(readFileSync(file, "utf8"));
    spec.formats.push("aider-conventions"); // makes Aider installable
    writeFileSync(file, JSON.stringify(spec, null, 2) + "\n");
    expectFailure(dir, /landing badge missing: Aider/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("plugin tools and unrendered formats derive as non-installable", () => {
  const tools = JSON.parse(readFileSync(join(ROOT, "src-tauri/data/tools.json"), "utf8")).tools;
  const formats = parseRenderFormats(
    JSON.parse(readFileSync(join(ROOT, "src-tauri/data/render-formats.json"), "utf8")),
  );
  const installable = installableTools(tools, formats);

  // Recognized-only: real upstream format, no native renderer.
  assert.ok(!installable.has("Aider"));
  assert.ok(!installable.has("Windsurf"));
  assert.ok(!installable.has("Mistral Vibe"));
  // Hermes is a plugin: installKind "plugin" is never per-agent installable.
  assert.ok(!installable.has("Hermes"));
  // Shipped renderers the old hardcoded lists omitted.
  assert.ok(installable.has("DeepSeek Harness"));
  assert.ok(installable.has("Oh My Pi"));
  assert.equal(installable.size, 13);
});

test("scopeLabel mirrors the registry's user/project caps", () => {
  assert.equal(scopeLabel({ scope: { user: true, project: true } }), "user + project");
  assert.equal(scopeLabel({ scope: { user: true, project: false } }), "user");
  assert.equal(scopeLabel({ scope: { user: false, project: true } }), "project");
  assert.equal(scopeLabel({}), "none");
});

test("badgeLabels survives nested badge divs", () => {
  const labels = badgeLabels(readFileSync(join(ROOT, "landing/index.html"), "utf8"));
  assert.ok(labels.length >= 2, "expected several badges");
  assert.ok(labels.includes("Claude Code"));
});

test("readmeRows ignores the header and separator", () => {
  const rows = readmeRows(readFileSync(join(ROOT, "README.md"), "utf8"));
  assert.ok(rows.length > 0);
  assert.ok(!rows.some((row) => row.tool === "Tool" || /^-+$/.test(row.tool)));
});
