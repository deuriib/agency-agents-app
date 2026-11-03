import { beforeEach, describe, expect, it, vi } from "vitest";

import { ruleInstall, rulePreview, ruleStatus, ruleSummary } from "$lib/test/fixtures";

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
