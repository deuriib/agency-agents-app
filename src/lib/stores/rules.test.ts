import { beforeEach, describe, expect, it, vi } from "vitest";

import { ruleInstall, rulePreview, ruleStatus, ruleSummary } from "$lib/test/fixtures";
import type { RuleTarget } from "$lib/types";

/**
 * The IPC boundary is the only thing stubbed: `invoke` is the app's single door
 * to the backend, so a test here proves the store's semantics without a Tauri
 * runtime, a corpus, or a filesystem (issue #62).
 */
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

/** A fresh module graph per test: the store singleton and its preview cache reset. */
async function freshStore() {
  vi.resetModules();
  const { rules } = await import("$lib/stores/rules.svelte");
  return rules;
}

beforeEach(() => {
  mocks.invoke.mockReset();
});

describe("load", () => {
  it("keeps only rule rows from the ledger", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "rules_status") return ruleStatus();
      if (command === "rules_list") return [ruleSummary()];
      if (command === "rules_installs") {
        return [
          ruleInstall({ tool: "piAgent" }),
          // An agent row shares installs.json; it must never reach the Rules UI.
          { ...ruleInstall({ tool: "cursor" }), domain: "agent" },
        ];
      }
      throw new Error(`unexpected command ${command}`);
    });

    await rules.load();

    expect(rules.installations).toHaveLength(1);
    expect(rules.installations[0].tool).toBe("piAgent");
    expect(rules.error).toBeNull();
    expect(rules.loaded).toBe(true);
  });

  it("clears the install truth when the read fails, instead of keeping stale rows", async () => {
    const rules = await freshStore();
    rules.installations = [ruleInstall()];
    rules.list = [ruleSummary()];
    rules.status = ruleStatus();
    mocks.invoke.mockRejectedValue(new Error("ledger unreadable"));

    await rules.load();

    // Fail closed: a state the app can no longer confirm must not be rendered as
    // installed, or the UI offers an Uninstall it cannot honour.
    expect(rules.installations).toEqual([]);
    expect(rules.list).toEqual([]);
    expect(rules.status).toBeNull();
    expect(rules.error).toBe("ledger unreadable");
    expect(rules.loaded).toBe(true);
    expect(rules.loading).toBe(false);
  });
});

describe("project scope without a folder", () => {
  it("install makes zero IPC calls and refuses", async () => {
    const rules = await freshStore();

    await expect(rules.install("coding-standards", "piAgent", "project", null)).rejects.toThrow(
      /project scope requires a project path/,
    );
    // The whole point: `project_path: null` would be a USER-scope write, so the
    // request must never leave the store.
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("uninstall makes zero IPC calls and refuses", async () => {
    const rules = await freshStore();

    await expect(rules.uninstall("coding-standards", "piAgent", "project", null)).rejects.toThrow(
      /project scope requires a project path/,
    );
    // Before #62's coverage, this path deleted the user-global rule and its row.
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("resolves to no install state at all", async () => {
    const rules = await freshStore();
    rules.installations = [ruleInstall({ scope: "user", projectPath: null })];

    expect(rules.installFor("coding-standards", "piAgent", "project", null)).toBeNull();
    expect(rules.installFor("coding-standards", "piAgent", "user", null)).not.toBeNull();
  });
});

describe("uninstall targets", () => {
  it("sends the picked project path for project scope", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "rules_uninstall") return null;
      if (command === "rules_installs") return [];
      throw new Error(`unexpected command ${command}`);
    });

    await rules.uninstall("coding-standards", "piAgent", "project", "/A");

    expect(mocks.invoke).toHaveBeenCalledWith("rules_uninstall", {
      slug: "coding-standards",
      harness: "piAgent",
      projectPath: "/A",
    });
  });

  it("sends a null project path for user scope", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "rules_uninstall") return null;
      if (command === "rules_installs") return [];
      throw new Error(`unexpected command ${command}`);
    });

    await rules.uninstall("coding-standards", "piAgent", "user", null);

    expect(mocks.invoke).toHaveBeenCalledWith("rules_uninstall", {
      slug: "coding-standards",
      harness: "piAgent",
      projectPath: null,
    });
  });
});

describe("preview cache", () => {
  it("serves a repeat compile from memory and refetches after an install", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "rules_preview") return rulePreview();
      if (command === "rules_install") return ruleInstall();
      if (command === "rules_installs") return [ruleInstall()];
      throw new Error(`unexpected command ${command}`);
    });

    await rules.preview("coding-standards", null);
    await rules.preview("coding-standards", null);
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "rules_preview")).toHaveLength(1);

    await rules.install("coding-standards", "piAgent", "user", null);
    await rules.preview("coding-standards", null);
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "rules_preview")).toHaveLength(2);
  });

  it("drops a late preview for a superseded request", async () => {
    const rules = await freshStore();
    let resolveFirst: (value: unknown) => void = () => {};
    mocks.invoke.mockImplementationOnce(
      () => new Promise((resolve) => (resolveFirst = resolve)),
    );
    mocks.invoke.mockImplementationOnce(async () =>
      rulePreview({ slug: "nexus-strategy", harnesses: [] }),
    );

    const first = rules.preview("coding-standards", null);
    const second = rules.preview("nexus-strategy", null);
    await second;
    resolveFirst(rulePreview());
    await first;

    expect(rules.previewResult?.slug).toBe("nexus-strategy");
  });
});

describe("selection race", () => {
  it("ignores a body that resolves after the selection changed", async () => {
    const rules = await freshStore();
    let resolveLate: (value: unknown) => void = () => {};
    mocks.invoke
      .mockImplementationOnce(() => new Promise((resolve) => (resolveLate = resolve)))
      .mockResolvedValueOnce({ slug: "nexus-strategy", body: "current" });

    rules.select("coding-standards");
    rules.select("nexus-strategy");
    // The first selection's response lands after the second one won.
    resolveLate({ slug: "coding-standards", body: "late" });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(rules.detail?.slug).toBe("nexus-strategy");
  });
});

describe("install wire args", () => {
  it("writes through rules_install and then re-reads the ledger once", async () => {
    const rules = await freshStore();
    const order: string[] = [];
    mocks.invoke.mockImplementation(async (command: string) => {
      order.push(command);
      if (command === "rules_install") return ruleInstall();
      if (command === "rules_installs") return [ruleInstall()];
      throw new Error(`unexpected command ${command}`);
    });

    await rules.install("coding-standards", "piAgent", "user", null);

    // Same triple the inline path sends: `projectPath: null` is the user key.
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "rules_install", {
      slug: "coding-standards",
      harness: "piAgent",
      projectPath: null,
    });
    expect(order).toEqual(["rules_install", "rules_installs"]);
    expect(rules.installations).toHaveLength(1);
    expect(rules.busy).toBeNull();
  });
});

describe("apply", () => {
  const good: RuleTarget = {
    slug: "coding-standards",
    harness: "piAgent",
    scope: "user",
    projectPath: null,
  };
  const bad: RuleTarget = {
    slug: "broken",
    harness: "piAgent",
    scope: "user",
    projectPath: null,
  };

  it("reports a partial failure and re-reads the ledger exactly once", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string, args: { slug?: string }) => {
      if (command === "rules_install") {
        if (args.slug === "broken") throw new Error("disk full");
        return ruleInstall();
      }
      if (command === "rules_installs") return [ruleInstall()];
      throw new Error(`unexpected command ${command}`);
    });

    const result = await rules.apply("install", [good, bad]);

    expect(result.ok).toBe(1);
    expect(result.fail).toBe(1);
    expect(result.failures).toEqual([{ target: bad, error: "disk full" }]);
    // Both writes were attempted, so the rejected target did not abort its
    // sibling; the ledger was read once for the whole action, not per target.
    expect(
      mocks.invoke.mock.calls.filter(([command]) => command === "rules_install"),
    ).toHaveLength(2);
    expect(
      mocks.invoke.mock.calls.filter(([command]) => command === "rules_installs"),
    ).toHaveLength(1);
    expect(rules.installations).toHaveLength(1);
    expect(rules.busy).toBeNull();
  });

  it("refuses a project scope without a root per target and writes nothing", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "rules_installs") return [];
      throw new Error(`unexpected command ${command}`);
    });

    const result = await rules.apply("install", [
      { slug: "coding-standards", harness: "piAgent", scope: "project", projectPath: null },
    ]);

    expect(result).toMatchObject({ ok: 0, fail: 1 });
    expect(result.failures[0].error).toMatch(/project scope requires a project path/);
    // `project_path: null` would be a user-scope write; it must never be sent.
    expect(
      mocks.invoke.mock.calls.filter(([command]) => command === "rules_install"),
    ).toHaveLength(0);
  });

  it("rejects while another operation holds the store", async () => {
    const rules = await freshStore();
    rules.busy = "coding-standards\u0000piAgent\u0000user\u0000";

    // A success-shaped no-op would be indistinguishable from "wrote everything".
    await expect(rules.apply("install", [good])).rejects.toThrow(
      /another operation is in progress/,
    );
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("rejects when the ledger re-read fails after the writes", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "rules_install") return ruleInstall();
      if (command === "rules_installs") throw new Error("ledger unreadable");
      throw new Error(`unexpected command ${command}`);
    });

    // Fail closed: a write the app cannot confirm is not a success.
    await expect(rules.apply("install", [good])).rejects.toThrow(/ledger unreadable/);
    expect(rules.busy).toBeNull();
  });

  it("sends rules_uninstall for a remove action", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "rules_uninstall") return null;
      if (command === "rules_installs") return [];
      throw new Error(`unexpected command ${command}`);
    });

    const result = await rules.apply("uninstall", [good]);

    expect(result).toMatchObject({ ok: 1, fail: 0, failures: [] });
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "rules_uninstall", {
      slug: "coding-standards",
      harness: "piAgent",
      projectPath: null,
    });
    expect(
      mocks.invoke.mock.calls.filter(([command]) => command === "rules_install"),
    ).toHaveLength(0);
  });
});

describe("previewFor", () => {
  it("caches per (slug, project root) and never touches the detail pane state", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "rules_preview") return rulePreview();
      throw new Error(`unexpected command ${command}`);
    });

    const first = await rules.previewFor("coding-standards", null);
    const second = await rules.previewFor("coding-standards", null);

    expect(second).toBe(first);
    expect(
      mocks.invoke.mock.calls.filter(([command]) => command === "rules_preview"),
    ).toHaveLength(1);
    // The whole point of previewFor: the grid must not clobber the pane.
    expect(rules.previewResult).toBeNull();
    expect(rules.previewError).toBeNull();
    expect(rules.previewLoading).toBe(false);
  });

  it("keys the cache by project root", async () => {
    const rules = await freshStore();
    mocks.invoke.mockImplementation(async (command: string, args: { projectPath: string | null }) => {
      if (command === "rules_preview") {
        return rulePreview({ scope: "project", projectPath: args.projectPath });
      }
      throw new Error(`unexpected command ${command}`);
    });

    await rules.previewFor("coding-standards", "/A");
    await rules.previewFor("coding-standards", "/B");

    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "rules_preview", {
      slug: "coding-standards",
      projectPath: "/B",
    });
    expect(rules.previewResult).toBeNull();
  });
});

