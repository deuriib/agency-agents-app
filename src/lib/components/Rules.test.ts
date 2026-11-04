import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import Rules from "$lib/components/Rules.svelte";
import { installableHarnesses } from "$lib/data/ruleRegistry";
import { i18n } from "$lib/stores/i18n.svelte";
import { rules } from "$lib/stores/rules.svelte";
import { ruleInstall, rulePreview, ruleStatus, ruleSummary } from "$lib/test/fixtures";
import type { RuleSummary } from "$lib/types";

/**
 * One test per state the workspace claims to render distinctly (issue #62).
 *
 * `invoke` is the only stub: `Rules` calls `rules.load()` on mount, so the
 * backend answers are what put the component in a state. Nothing here knows
 * about Tauri, the corpus, or the filesystem.
 */
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

function answer({
  status = ruleStatus(),
  list = [] as RuleSummary[],
  installs = [] as ReturnType<typeof ruleInstall>[],
} = {}) {
  mocks.invoke.mockImplementation(
    async (command: string, args: Record<string, unknown> = {}) => {
      if (command === "rules_status") return status;
      if (command === "rules_list") return list;
      if (command === "rules_installs") return installs;
      // The detail pane's body fetch, issued once a row is selected.
      if (command === "rules_get") {
        const slug = String(args.slug);
        const summary = list.find((rule) => rule.slug === slug) ?? ruleSummary({ slug });
        return { ...summary, body: "# Body\n", sourceHash: "source", bodyHash: "body" };
      }
      // One compile per scope: the detail pane's preview and every grid row.
      if (command === "rules_preview") {
        const projectPath = (args.projectPath as string | null) ?? null;
        return rulePreview({
          slug: String(args.slug),
          scope: projectPath === null ? "user" : "project",
          projectPath,
        });
      }
      // `InstallModal` mounts `projects.refresh()`, which reconciles agents.
      if (command === "installs_reconcile") return [];
      throw new Error(`unexpected command ${command}`);
    },
  );
}

beforeEach(() => {
  mocks.invoke.mockReset();
  rules.loaded = false;
  rules.loading = false;
  rules.error = null;
  rules.status = null;
  rules.list = [];
  rules.installations = [];
  rules.selectedSlug = null;
  rules.detail = null;
  rules.detailLoading = false;
  rules.detailError = null;
  rules.previewResult = null;
  rules.previewLoading = false;
  rules.previewError = null;
  rules.query = "";
  rules.lens = "all";
  rules.categoryFilter = null;
  rules.busy = null;
  rules.refreshing = false;
});

describe("list states", () => {
  it("loading: shows the skeleton, not an empty state", async () => {
    mocks.invoke.mockImplementation(() => new Promise(() => {}));

    render(Rules);

    expect(screen.getByText(i18n.t("common.loading"))).toBeInTheDocument();
    expect(screen.queryByText(i18n.t("rules.empty.noRulesTitle"))).not.toBeInTheDocument();
  });

  it("error: shows the failure, never a fake empty catalog", async () => {
    mocks.invoke.mockRejectedValue(new Error("catalog unreadable"));

    render(Rules);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(i18n.t("rules.loadFailedTitle"));
    expect(alert).toHaveTextContent("catalog unreadable");
    expect(screen.queryByText(i18n.t("rules.empty.noRulesTitle"))).not.toBeInTheDocument();
  });

  it("notSynced: says the corpus is not synced, with its own CTA", async () => {
    answer({ status: ruleStatus({ emptyReason: "notSynced", count: 0 }), list: [] });

    render(Rules);

    expect(await screen.findByText(i18n.t("rules.empty.notSyncedTitle"))).toBeInTheDocument();
    expect(screen.getByText(i18n.t("rules.empty.notSyncedCta"))).toBeInTheDocument();
  });

  it("noRules: distinguishes an empty corpus from an unsynced one", async () => {
    answer({ status: ruleStatus({ emptyReason: "noRules", count: 0 }), list: [] });

    render(Rules);

    expect(await screen.findByText(i18n.t("rules.empty.noRulesTitle"))).toBeInTheDocument();
    expect(screen.queryByText(i18n.t("rules.empty.notSyncedTitle"))).not.toBeInTheDocument();
  });

  it("partial: keeps invalid rules visible and says how many", async () => {
    rules.categoryFilter = "base";
    answer({
      list: [
        ruleSummary({ slug: "coding-standards" }),
        ruleSummary({
          slug: "broken-rule",
          name: "Broken Rule",
          validation: { valid: false, errors: ["missing description"] },
        }),
      ],
      status: ruleStatus({ count: 2 }),
    });

    render(Rules);

    const banner = await screen.findByText(
      i18n.count(1, "rules.invalid.banner.one", "rules.invalid.banner.many"),
    );
    expect(banner).toBeInTheDocument();
    // The invalid rule is still listed — never hidden, only barred from install.
    expect(await screen.findByText("Broken Rule")).toBeInTheDocument();
  });

  it("landing: one row per category with its count, plus an all-rules row", async () => {
    answer({
      list: [
        ruleSummary({ slug: "coding-standards", category: "base" }),
        ruleSummary({ slug: "typescript", category: "stacks", name: "TypeScript" }),
        ruleSummary({ slug: "commit-conventions", category: "base", name: "Commit Conventions" }),
      ],
      status: ruleStatus({ count: 3 }),
    });

    render(Rules);

    // "All rules" is a destination, not a no-op: it carries the total.
    expect(await screen.findByRole("button", { name: /All rules/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Base/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Stacks/ })).toBeInTheDocument();
    // Counts descend, then by label: base (2) before stacks (1).
    const labels = screen.getAllByRole("button").map((b) => b.textContent ?? "");
    expect(labels.findIndex((t) => t.includes("Base"))).toBeLessThan(
      labels.findIndex((t) => t.includes("Stacks")),
    );
  });

  it("landing: picking a category drills into its rules", async () => {
    answer({
      list: [ruleSummary({ slug: "coding-standards", category: "base" })],
      status: ruleStatus({ count: 1 }),
    });

    render(Rules);
    const base = await screen.findByRole("button", { name: /Base/ });
    base.click();

    expect(await screen.findByRole("button", { name: /Coding Standards/ })).toBeInTheDocument();
  });

  it("lens: the install-state tabs count rules and narrow the list", async () => {
    rules.categoryFilter = "base";
    answer({
      list: [
        ruleSummary({ slug: "coding-standards", name: "Coding Standards" }),
        ruleSummary({ slug: "commit-conventions", name: "Commit Conventions" }),
      ],
      installs: [ruleInstall({ slug: "coding-standards", tool: "piAgent" })],
      status: ruleStatus({ count: 2 }),
    });

    render(Rules);

    expect(await screen.findByText("Commit Conventions")).toBeInTheDocument();
    const installedTab = screen.getByRole("tab", {
      name: new RegExp(i18n.t("rules.install.installed")),
    });
    expect(installedTab).toHaveTextContent("1");

    installedTab.click();

    expect(await screen.findByText("Coding Standards")).toBeInTheDocument();
    expect(screen.queryByText("Commit Conventions")).not.toBeInTheDocument();
  });

  it("refresh: the rescan control is always in the search row", async () => {
    answer({ list: [ruleSummary({ category: "base" })], status: ruleStatus({ count: 1 }) });

    render(Rules);

    expect(
      await screen.findByRole("button", { name: i18n.t("rules.empty.noRulesCta") }),
    ).toBeInTheDocument();
  });

  it("filteredToZero: blames the query, not the corpus", async () => {
    rules.query = "zzz-no-match";
    answer({ list: [ruleSummary()], status: ruleStatus({ count: 1 }) });

    render(Rules);

    expect(await screen.findByText(i18n.t("rules.filtered.none"))).toBeInTheDocument();
    expect(screen.queryByText(i18n.t("rules.empty.noRulesTitle"))).not.toBeInTheDocument();
  });
});

describe("per-rule install state", () => {
  it("installed: the row announces the install and the ledger drives it", async () => {
    rules.categoryFilter = "base";
    answer({
      list: [ruleSummary()],
      installs: [ruleInstall({ tool: "piAgent", scope: "user", projectPath: null })],
      status: ruleStatus({ count: 1 }),
    });

    render(Rules);

    const row = await screen.findByRole("button", { name: /Coding Standards/ });
    expect(row.getAttribute("aria-label")).toContain(i18n.t("rules.install.installed"));
  });

  it("not-installed: the same row without the install fragment", async () => {
    rules.categoryFilter = "base";
    answer({ list: [ruleSummary()], installs: [], status: ruleStatus({ count: 1 }) });

    render(Rules);

    const row = await screen.findByRole("button", { name: /Coding Standards/ });
    expect(row.getAttribute("aria-label")).not.toContain(i18n.t("rules.install.installed"));
  });
});

describe("install grid entry point (#92)", () => {
  it("selecting a rule reveals the header action, which opens the harness grid", async () => {
    rules.categoryFilter = "base";
    answer({ list: [ruleSummary()], status: ruleStatus({ count: 1 }) });

    render(Rules);

    // Before a selection there is no detail pane, so the row is the only match.
    await fireEvent.click(await screen.findByRole("button", { name: /Coding Standards/ }));

    // The accessible name carries the rule name; the visible label is the action.
    const action = await screen.findByRole("button", {
      name: i18n.t("rules.install.gridTitle", { rule: "Coding Standards" }),
    });
    expect(action).toBeEnabled();
    expect(action).toHaveTextContent(i18n.t("rules.install.gridOpen"));

    await fireEvent.click(action);

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent(
      i18n.t("rules.install.gridTitle", { rule: "Coding Standards" }),
    );
    // Columns are exactly the four installable harnesses (`ruleRegistry`).
    for (const harness of installableHarnesses()) {
      expect(dialog).toHaveTextContent(harness.label);
    }
  });

  it("an invalid rule disables the action and states the reason", async () => {
    rules.categoryFilter = "base";
    answer({
      list: [
        ruleSummary({
          slug: "broken-rule",
          name: "Broken Rule",
          validation: { valid: false, errors: ["missing description"] },
        }),
      ],
      status: ruleStatus({ count: 1 }),
    });

    render(Rules);

    await fireEvent.click(await screen.findByRole("button", { name: /Broken Rule/ }));

    const reason = i18n.t("rules.install.reasonInvalid");
    const action = await screen.findByRole("button", {
      name: `${i18n.t("rules.install.gridTitle", { rule: "Broken Rule" })} — ${reason}`,
    });
    expect(action).toBeDisabled();
    expect(action).toHaveAttribute("title", reason);
    expect(action.getAttribute("aria-label")).toContain(reason);
    // The disabled control still names the rule it belongs to.
    expect(action.getAttribute("aria-label")).toContain("Broken Rule");
  });
});
