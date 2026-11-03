import { describe, expect, it } from "vitest";

import {
  effectiveRoot,
  filterRules,
  filtersActive,
  installBlockReason,
  installFor,
  installedHarnesses,
  installsFor,
  invalidCount,
  needsProjectRoot,
  previewEntry,
  ruleCategories,
  ruleListState,
} from "$lib/rules/derive";
import { ruleInstall, rulePreview, ruleStatus, ruleSummary } from "$lib/test/fixtures";

describe("effectiveRoot", () => {
  it("uses the project path for project scope and null for user scope", () => {
    expect(effectiveRoot("project", "/A")).toBe("/A");
    expect(effectiveRoot("user", null)).toBeNull();
    // A project scope that somehow carries a path still resolves to that path;
    // user scope never adopts one.
    expect(effectiveRoot("user", "/A")).toBeNull();
  });

  it("returns null for a project scope with no folder — never the user key", () => {
    expect(effectiveRoot("project", null)).toBeNull();
    expect(needsProjectRoot("project", null)).toBe(true);
    expect(needsProjectRoot("project", "/A")).toBe(false);
    expect(needsProjectRoot("user", null)).toBe(false);
  });
});

describe("installFor", () => {
  const userRow = ruleInstall({ scope: "user", projectPath: null, dest: "/home/u.md" });
  const projectARow = ruleInstall({ scope: "project", projectPath: "/A", dest: "/A/u.md" });

  it("never answers a project-scope question with the user row", () => {
    // The defect #62 pins: `projectPath: null` is the user ledger key, so a
    // project scope with no folder must resolve to no state at all.
    expect(installFor([userRow], "coding-standards", "piAgent", "project", null)).toBeNull();
  });

  it("returns the row for the picked project, not the user row", () => {
    const rows = [userRow, projectARow];
    const found = installFor(rows, "coding-standards", "piAgent", "project", "/A");
    expect(found).toBe(projectARow);
    expect(found?.dest).toBe("/A/u.md");

    expect(installFor(rows, "coding-standards", "piAgent", "project", "/B")).toBeNull();
  });

  it("returns the user row for user scope", () => {
    expect(installFor([userRow, projectARow], "coding-standards", "piAgent", "user", null)).toBe(
      userRow,
    );
  });

  it("is per (slug, harness, scope): another harness or rule does not match", () => {
    expect(installFor([userRow], "coding-standards", "ohMyPi", "user", null)).toBeNull();
    expect(installFor([userRow], "nexus-strategy", "piAgent", "user", null)).toBeNull();
  });
});

describe("installedHarnesses", () => {
  it("collects every harness across scopes for one rule only", () => {
    const rows = [
      ruleInstall({ tool: "piAgent", scope: "user" }),
      ruleInstall({ tool: "opencode", scope: "project", projectPath: "/A" }),
      ruleInstall({ slug: "nexus-strategy", tool: "antigravity" }),
    ];
    expect([...installedHarnesses(rows, "coding-standards")].sort()).toEqual([
      "opencode",
      "piAgent",
    ]);
    expect(installedHarnesses(rows, "missing").size).toBe(0);
  });
});

describe("installsFor", () => {
  it("orders by harness, then user before project, then by path", () => {
    const rows = [
      ruleInstall({ tool: "piAgent", scope: "project", projectPath: "/B" }),
      ruleInstall({ tool: "piAgent", scope: "user" }),
      ruleInstall({ tool: "piAgent", scope: "project", projectPath: "/A" }),
      ruleInstall({ tool: "antigravity", scope: "user" }),
    ];
    const ordered = installsFor(rows, "coding-standards").map(
      (row) => `${row.tool}:${row.scope}:${row.projectPath ?? "-"}`,
    );
    expect(ordered).toEqual([
      "antigravity:user:-",
      "piAgent:user:-",
      "piAgent:project:/A",
      "piAgent:project:/B",
    ]);
  });
});

describe("filterRules", () => {
  const list = [
    ruleSummary({ slug: "coding-standards", name: "Coding Standards", category: "base" }),
    ruleSummary({
      slug: "typescript",
      name: "TypeScript",
      description: "Typed JavaScript",
      category: "stacks",
    }),
  ];
  const installations = [ruleInstall({ slug: "typescript", tool: "opencode" })];

  it("returns everything when no filter is set", () => {
    expect(filterRules(list, [], { query: "", harnessFilter: null, categoryFilter: null })).toHaveLength(2);
    expect(filtersActive({ query: "  ", harnessFilter: null, categoryFilter: null })).toBe(false);
  });

  it("matches name, description and slug case-insensitively", () => {
    const byName = filterRules(list, [], { query: "CODING", harnessFilter: null, categoryFilter: null });
    expect(byName.map((r) => r.slug)).toEqual(["coding-standards"]);

    const byDescription = filterRules(list, [], {
      query: "javascript",
      harnessFilter: null,
      categoryFilter: null,
    });
    expect(byDescription.map((r) => r.slug)).toEqual(["typescript"]);

    const bySlug = filterRules(list, [], { query: "stacks", harnessFilter: null, categoryFilter: null });
    expect(bySlug.map((r) => r.slug)).toEqual([]);
  });

  it("composes the harness lens (from the ledger) with the category filter", () => {
    const harnessOnly = filterRules(list, installations, {
      query: "",
      harnessFilter: "opencode",
      categoryFilter: null,
    });
    expect(harnessOnly.map((r) => r.slug)).toEqual(["typescript"]);

    const contradictory = filterRules(list, installations, {
      query: "",
      harnessFilter: "opencode",
      categoryFilter: "base",
    });
    expect(contradictory).toEqual([]);

    expect(filtersActive({ query: "", harnessFilter: "opencode", categoryFilter: null })).toBe(true);
  });
});

describe("categories and invalid count", () => {
  it("sorts categories deterministically and counts only invalid rules", () => {
    const list = [
      ruleSummary({ category: "stacks" }),
      ruleSummary({ category: "base" }),
      ruleSummary({ category: "base", validation: { valid: false, errors: ["boom"] } }),
    ];
    expect(ruleCategories(list)).toEqual(["base", "stacks"]);
    expect(invalidCount(list)).toBe(1);
  });
});

describe("previewEntry", () => {
  const entry = { harness: "piAgent" };

  it("answers only for the scope and root the preview was compiled for", () => {
    const userPreview = rulePreview({ scope: "user", projectPath: null });
    expect(previewEntry(userPreview, "coding-standards", "piAgent", "user", null)?.harness).toBe(
      entry.harness,
    );
    // A user-scope preview must never answer a project-scope question.
    expect(previewEntry(userPreview, "coding-standards", "piAgent", "project", "/A")).toBeNull();

    const projectPreview = rulePreview({ scope: "project", projectPath: "/A" });
    expect(previewEntry(projectPreview, "coding-standards", "piAgent", "project", "/A")?.harness).toBe(
      entry.harness,
    );
    expect(previewEntry(projectPreview, "coding-standards", "piAgent", "project", "/B")).toBeNull();
  });

  it("rejects another rule's preview and an unknown harness", () => {
    const preview = rulePreview();
    expect(previewEntry(preview, "nexus-strategy", "piAgent", "user", null)).toBeNull();
    expect(previewEntry(preview, "coding-standards", "antigravity", "user", null)).toBeNull();
    expect(previewEntry(null, "coding-standards", "piAgent", "user", null)).toBeNull();
  });
});

describe("installBlockReason", () => {
  const allowed = {
    valid: true,
    harness: "piAgent",
    installable: true,
    scope: "user" as const,
    projectPath: null,
    previewLoading: false,
    previewError: null,
    dest: "/home/tester/rule.md",
  };

  it("allows the install when every gate passes", () => {
    expect(installBlockReason(allowed)).toBeNull();
  });

  it("orders the reasons most-specific-first", () => {
    // invalid beats no-root
    expect(
      installBlockReason({ ...allowed, valid: false, scope: "project", projectPath: null }),
    ).toBe("invalid");
    // no-root beats not-installable
    expect(
      installBlockReason({
        ...allowed,
        scope: "project",
        projectPath: null,
        harness: null,
        installable: false,
      }),
    ).toBe("noProjectRoot");
    // not-installable beats preview loading
    expect(
      installBlockReason({ ...allowed, harness: null, previewLoading: true }),
    ).toBe("notInstallable");
    // preview loading beats preview failed (a load cannot have both, but the
    // precedence must be deterministic if it does)
    expect(
      installBlockReason({ ...allowed, previewLoading: true, previewError: "boom" }),
    ).toBe("previewLoading");
    expect(installBlockReason({ ...allowed, previewError: "boom" })).toBe("previewFailed");
    // settled, no error, no destination
    expect(installBlockReason({ ...allowed, dest: null })).toBe("notInstallable");
    // an uninstallable harness is generic, not "invalid"
    expect(installBlockReason({ ...allowed, installable: false })).toBe("notInstallable");
  });
});

describe("ruleListState", () => {
  const base = {
    loaded: true,
    loading: false,
    error: null,
    listLength: 2,
    status: ruleStatus(),
    visibleLength: 2,
  };

  it("distinguishes not-fetched-yet from every empty arm", () => {
    expect(ruleListState({ ...base, loaded: false }).kind).toBe("loading");
    expect(ruleListState({ ...base, loading: true }).kind).toBe("loading");
    expect(ruleListState({ ...base, error: "boom" })).toEqual({ kind: "error", message: "boom" });
    expect(
      ruleListState({ ...base, listLength: 0, visibleLength: 0, status: ruleStatus({ emptyReason: "notSynced" }) })
        .kind,
    ).toBe("notSynced");
    expect(
      ruleListState({ ...base, listLength: 0, visibleLength: 0, status: ruleStatus({ emptyReason: "noRules" }) })
        .kind,
    ).toBe("noRules");
    expect(ruleListState({ ...base, listLength: 0, visibleLength: 0, status: null }).kind).toBe(
      "noRules",
    );
    expect(ruleListState({ ...base, visibleLength: 0 }).kind).toBe("filteredToZero");
    expect(ruleListState(base).kind).toBe("rows");
  });

  it("keeps an error visible even when a stale list is still in memory", () => {
    expect(ruleListState({ ...base, error: "boom", listLength: 5 }).kind).toBe("error");
  });
});
