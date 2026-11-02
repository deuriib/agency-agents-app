#!/usr/bin/env node
// Keep the documented tool lists honest.
//
// Two hand-written lists claim what the app can install:
//   - landing/index.html  → the "Installs into the tools you already use" badges
//   - README.md           → the "Supported Install Targets" table
//
// Both drifted. The landing page still badged eight tools and called Antigravity
// "needs additional app work" long after it shipped a renderer, while DeepSeek
// Harness and Oh My Pi were installable and absent. The README had the same
// eight-plus-three set and a "Scope Today" column that contradicted the registry
// (Claude Code has supported project scope for a while).
//
// The installable set is derived the way the app derives it:
//
//     tools.json `format`  ∩  render-formats.json, minus `plugin`
//
// `src-tauri/data/render-formats.json` is the single source of truth, embedded
// by the Rust registry and imported by the frontend. `singleSourceWiring`
// asserts both readers still read it, so a reintroduced literal list fails here
// instead of drifting silently (issues #49, #53).
//
// `scope` comes straight from tools.json, which is what render::supports_user
// and render::supports_project read.

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** The shared renderer capability list, validated. */
export function parseRenderFormats(spec) {
  if (!Array.isArray(spec?.formats) || spec.formats.length === 0) {
    throw new Error("data/render-formats.json: `formats` must be a non-empty array");
  }
  const formats = new Set(spec.formats);
  if (formats.size !== spec.formats.length) {
    throw new Error("data/render-formats.json: duplicate format entries");
  }
  return formats;
}

/** Both sides must read the shared file rather than restate the list. */
export function singleSourceWiring(root) {
  const registryRs = readFileSync(join(root, "src-tauri/src/registry.rs"), "utf8");
  const toolRegistryTs = readFileSync(join(root, "src/lib/data/toolRegistry.ts"), "utf8");
  return {
    rust: registryRs.includes('include_str!("../data/render-formats.json")'),
    frontend: toolRegistryTs.includes('"../../../src-tauri/data/render-formats.json"'),
  };
}

/** Tools this app can install, keyed by label. */
export function installableTools(tools, formats) {
  const out = new Map();
  for (const tool of Object.values(tools)) {
    if (tool.installKind === "plugin") continue;
    if (!formats.has(tool.format)) continue;
    out.set(tool.label, tool);
  }
  return out;
}

/** How the docs spell a tool's scope caps. */
export function scopeLabel(tool) {
  const user = tool.scope?.user ?? false;
  const project = tool.scope?.project ?? false;
  if (user && project) return "user + project";
  if (user) return "user";
  if (project) return "project";
  return "none";
}

/** `<code>` labels rendered inside the landing page's .badges grid. */
export function badgeLabels(html) {
  const start = html.indexOf('<div class="badges">');
  if (start === -1) throw new Error('landing/index.html has no <div class="badges"> block');
  // The grid is followed by the explanatory note. Anchoring the end on that <p>
  // keeps the nested badge divs from truncating the slice at the first </div>.
  const end = html.indexOf('<p class="small-note">', start);
  if (end === -1) throw new Error('the .badges block is not followed by <p class="small-note">');
  return [...html.slice(start, end).matchAll(/<code>([^<]+)<\/code>/g)].map((match) => match[1]);
}

/** Rows of the README's "Supported Install Targets" table as { tool, scope }. */
export function readmeRows(markdown) {
  const start = markdown.indexOf("## Supported Install Targets");
  if (start === -1) throw new Error('README.md has no "## Supported Install Targets" section');
  const nextSection = markdown.indexOf("\n## ", start + 1);
  const section = markdown.slice(start, nextSection === -1 ? undefined : nextSection);

  const rows = [];
  for (const line of section.split("\n")) {
    if (!line.trim().startsWith("|")) continue;
    const cells = line.split("|").slice(1, -1).map((cell) => cell.trim());
    if (cells.length < 2) continue;
    if (cells[0] === "Tool" || /^:?-+:?$/.test(cells[0])) continue; // header + separator
    rows.push({ tool: cells[0], scope: cells[1] });
  }
  return rows;
}

/** Compare both documented lists against the registry-derived truth. */
export function compare(root) {
  const tools = JSON.parse(readFileSync(join(root, "src-tauri/data/tools.json"), "utf8")).tools;
  const formats = parseRenderFormats(
    JSON.parse(readFileSync(join(root, "src-tauri/data/render-formats.json"), "utf8")),
  );
  const wiring = singleSourceWiring(root);
  const installable = installableTools(tools, formats);
  const expected = [...installable.keys()].sort();

  const badged = badgeLabels(readFileSync(join(root, "landing/index.html"), "utf8"));
  const rows = readmeRows(readFileSync(join(root, "README.md"), "utf8"));

  const badgedSet = new Set(badged);
  const rowSet = new Set(rows.map((row) => row.tool));

  return {
    expected,
    badged,
    rows,
    wiring,
    badgeMissing: expected.filter((label) => !badgedSet.has(label)),
    badgeStale: badged.filter((label) => !installable.has(label)),
    badgeDuplicates: badged.filter((label, index) => badged.indexOf(label) !== index),
    readmeMissing: expected.filter((label) => !rowSet.has(label)),
    readmeStale: rows.map((row) => row.tool).filter((label) => !installable.has(label)),
    readmeScopeWrong: rows
      .filter((row) => installable.has(row.tool) && row.scope !== scopeLabel(installable.get(row.tool)))
      .map((row) => `${row.tool}: says "${row.scope}", registry says "${scopeLabel(installable.get(row.tool))}"`),
  };
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  const root = process.argv[2]
    ? resolve(process.argv[2])
    : resolve(dirname(fileURLToPath(import.meta.url)), "..");

  let result;
  try {
    result = compare(root);
  } catch (error) {
    console.error(`check-doc-tools: ${error.message}`);
    process.exit(2);
  }

  const problems = [];
  if (!result.wiring.rust) {
    problems.push("src-tauri/src/registry.rs no longer embeds data/render-formats.json — a literal list is back");
  }
  if (!result.wiring.frontend) {
    problems.push("src/lib/data/toolRegistry.ts no longer imports data/render-formats.json — a literal list is back");
  }
  if (result.badgeMissing.length) {
    problems.push(`landing badge missing: ${result.badgeMissing.join(", ")}`);
  }
  if (result.badgeStale.length) {
    problems.push(`landing badge not installable: ${result.badgeStale.join(", ")}`);
  }
  if (result.badgeDuplicates.length) {
    problems.push(`landing badge duplicated: ${result.badgeDuplicates.join(", ")}`);
  }
  if (result.readmeMissing.length) {
    problems.push(`README row missing: ${result.readmeMissing.join(", ")}`);
  }
  if (result.readmeStale.length) {
    problems.push(`README row not installable: ${result.readmeStale.join(", ")}`);
  }
  if (result.readmeScopeWrong.length) {
    problems.push(`README scope wrong: ${result.readmeScopeWrong.join("; ")}`);
  }

  if (problems.length) {
    console.error("check-doc-tools: documented tool lists are out of sync with the tool registry");
    for (const problem of problems) console.error(`  - ${problem}`);
    console.error(`  expected ${result.expected.length}: ${result.expected.join(", ")}`);
    process.exit(1);
  }

  console.log(
    `check-doc-tools: ${result.badged.length} badges and ${result.rows.length} README rows match the registry`,
  );
}
