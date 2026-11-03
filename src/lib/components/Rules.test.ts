import { render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import Rules from "$lib/components/Rules.svelte";
import { i18n } from "$lib/stores/i18n.svelte";
import { rules } from "$lib/stores/rules.svelte";
import { ruleInstall, ruleStatus, ruleSummary } from "$lib/test/fixtures";
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
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === "rules_status") return status;
    if (command === "rules_list") return list;
    if (command === "rules_installs") return installs;
    throw new Error(`unexpected command ${command}`);
  });
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
  rules.harnessFilter = null;
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
    answer({ list: [ruleSummary()], installs: [], status: ruleStatus({ count: 1 }) });

    render(Rules);

    const row = await screen.findByRole("button", { name: /Coding Standards/ });
    expect(row.getAttribute("aria-label")).not.toContain(i18n.t("rules.install.installed"));
  });
});
