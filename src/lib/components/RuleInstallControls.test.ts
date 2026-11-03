import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import RuleInstallControls from "$lib/components/RuleInstallControls.svelte";
import { i18n } from "$lib/stores/i18n.svelte";
import { rules } from "$lib/stores/rules.svelte";
import { ruleInstall } from "$lib/test/fixtures";
import type { Scope } from "$lib/types";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

function renderControls(
  overrides: Partial<{
    valid: boolean;
    harness: string | null;
    scope: Scope;
    projectPath: string | null;
    dest: string | null;
    previewLoading: boolean;
    previewError: string | null;
  }> = {},
) {
  return render(RuleInstallControls, {
    slug: "coding-standards",
    name: "Coding Standards",
    valid: true,
    harness: "piAgent",
    scope: "user" as Scope,
    projectPath: null,
    dest: "/home/tester/.pi/agent/rules/coding-standards/RULES.md",
    previewLoading: false,
    previewError: null,
    ...overrides,
  });
}

beforeEach(() => {
  rules.installations = [];
  rules.busy = null;
  mocks.invoke.mockReset();
});

describe("install state per (scope, root)", () => {
  it("renders no install state at all for a project scope with no folder", async () => {
    rules.installations = [ruleInstall({ scope: "user", projectPath: null })];

    renderControls({ scope: "project", projectPath: null });

    // The user row shares `project_path: null`; resolving it here would offer an
    // Uninstall that deletes the user-global rule (the #61 review defect).
    expect(screen.getByText(i18n.t("rules.install.reasonNoProjectRoot"))).toBeInTheDocument();
    expect(screen.queryByText(i18n.t("rules.uninstall.action"))).not.toBeInTheDocument();
    expect(screen.queryByText(i18n.t("rules.install.installed"))).not.toBeInTheDocument();
  });

  it("shows the project row — never the user row — for project + a picked root", async () => {
    rules.installations = [
      ruleInstall({ scope: "user", projectPath: null, dest: "/home/tester/.pi/user.md" }),
      ruleInstall({ scope: "project", projectPath: "/A", dest: "/A/.pi/project.md" }),
    ];

    renderControls({ scope: "project", projectPath: "/A" });

    expect(screen.getByText(i18n.t("rules.install.installed"))).toBeInTheDocument();
    expect(screen.getByText("/A/.pi/project.md")).toBeInTheDocument();
    expect(screen.queryByText("/home/tester/.pi/user.md")).not.toBeInTheDocument();
    expect(screen.getByText(i18n.t("rules.uninstall.action"))).toBeInTheDocument();
  });

  it("shows the user row for user scope", async () => {
    rules.installations = [
      ruleInstall({ scope: "user", projectPath: null, dest: "/home/tester/.pi/user.md" }),
    ];

    renderControls({ scope: "user", projectPath: null });

    expect(screen.getByText("/home/tester/.pi/user.md")).toBeInTheDocument();
    expect(screen.getByText(i18n.t("rules.install.installed"))).toBeInTheDocument();
  });
});

describe("uninstall never fires for a project scope with no root", () => {
  it("renders no uninstall button, so no IPC can be triggered", async () => {
    rules.installations = [ruleInstall({ scope: "user", projectPath: null })];

    renderControls({ scope: "project", projectPath: null });

    expect(screen.queryByRole("button", { name: new RegExp(i18n.t("rules.uninstall.action")) })).toBeNull();
    fireEvent.click(screen.getByText(i18n.t("rules.install.reasonNoProjectRoot")));
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});

describe("disabled-install reason precedence", () => {
  it("reports an invalid rule and disables install", async () => {
    renderControls({ valid: false });

    expect(screen.getByText(i18n.t("rules.install.reasonInvalid"))).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: new RegExp(i18n.t("rules.install.action")) }),
    ).toBeDisabled();
  });

  it("shows only the missing-root sentence when a project folder is unpicked", async () => {
    // No install button exists in this state, so the reason line is the only
    // thing rendered — the invalid/not-installable gates never get a chance to
    // speak, which is why the pure precedence is pinned in derive.test.ts too.
    renderControls({ valid: false, scope: "project", projectPath: null });

    expect(screen.getByText(i18n.t("rules.install.reasonNoProjectRoot"))).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: new RegExp(i18n.t("rules.install.action")) })).toBeNull();
  });

  it("reports a missing project root before an unwritable harness", async () => {
    renderControls({ harness: null, scope: "project", projectPath: null });

    expect(screen.getByText(i18n.t("rules.install.reasonNoProjectRoot"))).toBeInTheDocument();
    expect(screen.queryByText(i18n.t("rules.install.reasonNotInstallable"))).not.toBeInTheDocument();
  });

  it("reports an unwritable harness before a loading preview", async () => {
    renderControls({ harness: null, previewLoading: true });

    expect(screen.getByText(i18n.t("rules.install.reasonNotInstallable"))).toBeInTheDocument();
    expect(screen.queryByText(i18n.t("rules.install.reasonPreviewLoading"))).not.toBeInTheDocument();
  });

  it("reports a loading preview before a failed one", async () => {
    renderControls({ previewLoading: true, previewError: "boom" });

    expect(screen.getByText(i18n.t("rules.install.reasonPreviewLoading"))).toBeInTheDocument();
    expect(screen.queryByText(i18n.t("rules.install.reasonPreviewFailed"))).not.toBeInTheDocument();
  });

  it("reports a failed preview when the compile settled with an error", async () => {
    renderControls({ previewError: "boom" });

    expect(screen.getByText(i18n.t("rules.install.reasonPreviewFailed"))).toBeInTheDocument();
  });

  it("reports no destination as unwritable when the compile settled with nothing", async () => {
    renderControls({ dest: null });

    expect(screen.getByText(i18n.t("rules.install.reasonNotInstallable"))).toBeInTheDocument();
  });

  it("enables the install when every gate passes", async () => {
    renderControls();

    const install = screen.getByRole("button", { name: new RegExp(i18n.t("rules.install.action")) });
    expect(install).toBeEnabled();
    expect(screen.queryByText(i18n.t("rules.install.reasonInvalid"))).not.toBeInTheDocument();
  });
});
