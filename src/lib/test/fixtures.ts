/**
 * Test fixtures for the Rules workspace.
 *
 * Factories, not shared objects: a test that mutates one field must not leak
 * into the next case. Every default is a valid value, so a test only states the
 * field it cares about.
 */
import type {
  RuleInstall,
  RulePreview,
  RulePreviewEntry,
  RuleStatus,
  RuleSummary,
} from "$lib/types";

export function ruleSummary(overrides: Partial<RuleSummary> = {}): RuleSummary {
  return {
    slug: "coding-standards",
    name: "Coding Standards",
    description: "House style for every language.",
    category: "base",
    relativePath: "rules/base/coding-standards.md",
    tags: [],
    tools: [],
    validation: { valid: true, errors: [] },
    ...overrides,
  };
}

export function ruleStatus(overrides: Partial<RuleStatus> = {}): RuleStatus {
  return {
    configured: true,
    count: 1,
    version: "baseline",
    emptyReason: null,
    ...overrides,
  };
}

/** A rule ledger row: `domain: "rule"` is what narrows `InstallRecord` to `RuleInstall`. */
export function ruleInstall(overrides: Partial<RuleInstall> = {}): RuleInstall {
  return {
    slug: "coding-standards",
    tool: "piAgent",
    scope: "user",
    projectPath: null,
    dest: "/home/tester/.pi/agent/rules/coding-standards/RULES.md",
    sourceHash: "source",
    bodyHash: "body",
    renderedHash: "rendered",
    installedAt: "2026-11-03T00:00:00Z",
    corpusVersion: "baseline",
    domain: "rule",
    ...overrides,
  };
}

export function previewEntry(overrides: Partial<RulePreviewEntry> = {}): RulePreviewEntry {
  return {
    harness: "piAgent",
    label: "Pi Coding Agent",
    bytes: "# Coding Standards\n",
    dest: "/home/tester/.pi/agent/rules/coding-standards/RULES.md",
    renderedHash: "rendered",
    installable: true,
    ...overrides,
  };
}

export function rulePreview(overrides: Partial<RulePreview> = {}): RulePreview {
  return {
    slug: "coding-standards",
    scope: "user",
    projectPath: null,
    harnesses: [previewEntry()],
    ...overrides,
  };
}
