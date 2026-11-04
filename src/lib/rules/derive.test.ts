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
  ruleCategoryTiles,
  ruleCellCoverage,
  ruleCellTargets,
  ruleLensCounts,
  ruleListState,
} from "$lib/rules/derive";
import { declaredRuleCategories, ruleCategoryMeta } from "$lib/data/ruleCategories";
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

describe("ruleCellCoverage", () => {
  const rowA = ruleInstall({ slug: "a", tool: "piAgent", scope: "user", projectPath: null });
  const rowB = ruleInstall({ slug: "b", tool: "piAgent", scope: "user", projectPath: null });

  it("is all for one installed slug and neither all nor some for one missing slug", () => {
    const on = ruleCellCoverage([rowA], ["a"], "piAgent", "user", null);
    expect(on).toMatchObject({ present: 1, total: 1, all: true, some: false });
    expect(on.rows).toEqual([rowA]);

    const off = ruleCellCoverage([rowA], ["b"], "piAgent", "user", null);
    expect(off).toMatchObject({ present: 0, total: 1, all: false, some: false });
    expect(off.rows).toEqual([]);
  });

  it("distinguishes all / some / none across two slugs", () => {
    const all = ruleCellCoverage([rowA, rowB], ["a", "b"], "piAgent", "user", null);
    expect(all).toMatchObject({ present: 2, total: 2, all: true, some: false });
    expect(all.rows.map((row) => row.slug)).toEqual(["a", "b"]);

    const some = ruleCellCoverage([rowA], ["a", "b"], "piAgent", "user", null);
    expect(some).toMatchObject({ present: 1, total: 2, all: false, some: true });

    const none = ruleCellCoverage([], ["a", "b"], "piAgent", "user", null);
    expect(none).toMatchObject({ present: 0, total: 2, all: false, some: false });
  });

  it("pins #62: a project scope with no root has no state, even with a user row present", () => {
    const coverage = ruleCellCoverage([rowA], ["a"], "piAgent", "project", null);
    // Adopting the user row here is how the UI once offered a delete of the
    // user-global files; the no-root cell must stay empty.
    expect(coverage).toMatchObject({ present: 0, total: 1, all: false, some: false });
    expect(coverage.rows).toEqual([]);
  });

  it("counts a duplicated slug once so all stays reachable", () => {
    const coverage = ruleCellCoverage([rowA], ["a", "a"], "piAgent", "user", null);
    expect(coverage).toMatchObject({ present: 1, total: 1, all: true, some: false });
    expect(coverage.rows).toHaveLength(1);
  });

  it("is neither all nor some when no slug is asked about", () => {
    const coverage = ruleCellCoverage([rowA], [], "piAgent", "user", null);
    expect(coverage).toMatchObject({ present: 0, total: 0, all: false, some: false });
  });

  it("does not let another harness, scope or project answer the cell", () => {
    const rows = [
      ruleInstall({ slug: "a", tool: "opencode", scope: "user" }),
      ruleInstall({ slug: "a", tool: "piAgent", scope: "project", projectPath: "/B" }),
    ];
    expect(ruleCellCoverage(rows, ["a"], "piAgent", "user", null).present).toBe(0);
    expect(ruleCellCoverage(rows, ["a"], "opencode", "project", "/B").present).toBe(0);
  });
});

describe("ruleCellTargets", () => {
  it("returns exactly the slugs missing for that cell, with scope and root", () => {
    const rows = [ruleInstall({ slug: "a", tool: "piAgent", scope: "user", projectPath: null })];
    expect(ruleCellTargets(rows, ["a", "b"], "piAgent", "user", null)).toEqual([
      { slug: "b", harness: "piAgent", scope: "user", projectPath: null },
    ]);
  });

  it("keeps the picked project root and never adopts another project's row", () => {
    const rows = [
      ruleInstall({ slug: "a", tool: "opencode", scope: "project", projectPath: "/B" }),
    ];
    expect(ruleCellTargets(rows, ["a"], "opencode", "project", "/A")).toEqual([
      { slug: "a", harness: "opencode", scope: "project", projectPath: "/A" },
    ]);
  });

  it("is the complement of coverage.present for every slug set", () => {
    const rows = [ruleInstall({ slug: "a" }), ruleInstall({ slug: "b" })];
    for (const slugs of [["a"], ["a", "b"], ["a", "b", "c"], []]) {
      const coverage = ruleCellCoverage(rows, slugs, "piAgent", "user", null);
      const targets = ruleCellTargets(rows, slugs, "piAgent", "user", null);
      expect(targets).toHaveLength(slugs.length - coverage.present);
    }
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
    expect(filterRules(list, [], { query: "", categoryFilter: null, lens: "all" })).toHaveLength(2);
    expect(filtersActive({ query: "  ", categoryFilter: null, lens: "all" })).toBe(false);
  });

  it("matches name, description and slug case-insensitively", () => {
    const byName = filterRules(list, [], { query: "CODING", categoryFilter: null, lens: "all" });
    expect(byName.map((r) => r.slug)).toEqual(["coding-standards"]);

    const byDescription = filterRules(list, [], {
      query: "javascript",
      categoryFilter: null,
      lens: "all",
    });
    expect(byDescription.map((r) => r.slug)).toEqual(["typescript"]);

    const bySlug = filterRules(list, [], { query: "stacks", categoryFilter: null, lens: "all" });
    expect(bySlug.map((r) => r.slug)).toEqual([]);
  });

  it("composes the install-state lens with the category filter", () => {
    const installed = filterRules(list, installations, {
      query: "",
      categoryFilter: null,
      lens: "installed",
    });
    expect(installed.map((r) => r.slug)).toEqual(["typescript"]);

    const notInstalled = filterRules(list, installations, {
      query: "",
      categoryFilter: null,
      lens: "notInstalled",
    });
    expect(notInstalled.map((r) => r.slug)).toEqual(["coding-standards"]);

    const contradictory = filterRules(list, installations, {
      query: "",
      categoryFilter: "base",
      lens: "installed",
    });
    expect(contradictory).toEqual([]);

    expect(filtersActive({ query: "", categoryFilter: null, lens: "installed" })).toBe(true);
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

describe("ruleLensCounts", () => {
  it("counts installed and not-installed rules over the current list", () => {
    const list = [
      ruleSummary({ slug: "a" }),
      ruleSummary({ slug: "b" }),
      ruleSummary({ slug: "c" }),
    ];
    // Duplicate rows for the same rule (two harnesses) count once.
    const installations = [
      ruleInstall({ slug: "a", tool: "piAgent" }),
      ruleInstall({ slug: "a", tool: "opencode" }),
    ];
    expect(ruleLensCounts(list, installations)).toEqual({ all: 3, installed: 1, notInstalled: 2 });
    expect(ruleLensCounts([], [])).toEqual({ all: 0, installed: 0, notInstalled: 0 });
  });
});

describe("ruleCategoryTiles", () => {
  const meta = (slug: string) => ruleCategoryMeta(slug);

  it("orders by count desc, then label, then slug", () => {
    const list = [
      ruleSummary({ category: "stacks" }),
      ruleSummary({ category: "base" }),
      ruleSummary({ category: "base" }),
      ruleSummary({ category: "zeta" }),
      ruleSummary({ category: "zeta" }),
    ];
    const tiles = ruleCategoryTiles(list, meta);
    expect(tiles.map((t) => `${t.slug}:${t.count}`)).toEqual([
      "base:2",
      "zeta:2",
      "stacks:1",
    ]);
  });

  it("never drops a category the metadata does not describe", () => {
    const tiles = ruleCategoryTiles([ruleSummary({ category: "brand-new" })], meta);
    expect(tiles).toHaveLength(1);
    expect(tiles[0]).toMatchObject({
      slug: "brand-new",
      label: "Brand New",
      icon: "Layers",
      color: "#94A3B8",
      count: 1,
    });
  });

  it("uses the declared metadata for a shipped category", () => {
    const tiles = ruleCategoryTiles([ruleSummary({ category: "stacks" })], meta);
    expect(tiles[0].label).toBe("Stacks");
    expect(tiles[0].icon).toBe("Boxes");
    expect(tiles[0].description).toBeTruthy();
  });
});

describe("rules-categories.json", () => {
  it("describes every category the bundled corpus ships", () => {
    // The bundled baseline carries exactly these two categories; a new one must
    // still render (see the fallback test above), but it should be dressed up.
    expect(declaredRuleCategories()).toEqual(["base", "stacks"]);
  });

  it("gives every declared category a label, a bound icon and a hex colour", () => {
    for (const slug of declaredRuleCategories()) {
      const row = ruleCategoryMeta(slug);
      expect(row.label.length).toBeGreaterThan(0);
      expect(row.icon).toMatch(/^[A-Z][A-Za-z0-9]+$/);
      expect(row.color).toMatch(/^#[0-9A-Fa-f]{6}$/);
    }
  });
});
