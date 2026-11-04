import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import InstallModal from "$lib/components/InstallModal.svelte";
import { installableHarnesses } from "$lib/data/ruleRegistry";
import { install } from "$lib/stores/install.svelte";
import { i18n } from "$lib/stores/i18n.svelte";
import { projects } from "$lib/stores/projects.svelte";
import { rules } from "$lib/stores/rules.svelte";
import { toast } from "$lib/stores/toast.svelte";
import { previewEntry, ruleInstall, rulePreview, ruleSummary } from "$lib/test/fixtures";
import type { RuleInstall, RulePreview, Scope } from "$lib/types";

/**
 * The rules branch of the shared destinations × harnesses grid (issue #92).
 *
 * `invoke` is the only stub: the component derives every cell from the ledger
 * and from `rules_preview`, so answering those commands is what puts it in a
 * state. The agent branch has no test suite of its own; it is pinned by
 * Task 3's type-checked signature change plus the manual pass over its four
 * surfaces, NOT by this file.
 */
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

interface Backend {
  ledger: RuleInstall[];
  preview: (slug: string, projectPath: string | null) => RulePreview;
  install?: (slug: string, harness: string, projectPath: string | null) => void;
  uninstall?: (slug: string, harness: string, projectPath: string | null) => void;
}

/** A preview whose destinations are unique per harness/root, so cell labels can be composed. */
function previewData(slug: string, projectPath: string | null): RulePreview {
  const scope: Scope = projectPath === null ? "user" : "project";
  const root = projectPath ?? "~";
  return rulePreview({
    slug,
    scope,
    projectPath,
    harnesses: installableHarnesses().map((harness) =>
      previewEntry({
        harness: harness.id,
        label: harness.label,
        dest: `${root}/${harness.id}/${slug}.md`,
      }),
    ),
  });
}

/**
 * A preview that answers only SOME harnesses — two slugs can then own disjoint
 * halves of a row, so each cell's destination proves which slug's cache entry
 * answered it.
 */
function previewHalf(slug: string, projectPath: string | null, harnessIds: string[]): RulePreview {
  const scope: Scope = projectPath === null ? "user" : "project";
  const root = projectPath ?? "~";
  return rulePreview({
    slug,
    scope,
    projectPath,
    harnesses: installableHarnesses()
      .filter((harness) => harnessIds.includes(harness.id))
      .map((harness) =>
        previewEntry({
          harness: harness.id,
          label: harness.label,
          dest: `${root}/${harness.id}/${slug}.md`,
        }),
      ),
  });
}

function addInstall(
  backend: Backend,
  slug: string,
  harness: string,
  projectPath: string | null,
): void {
  backend.ledger = [
    ...backend.ledger,
    ruleInstall({
      slug,
      tool: harness,
      scope: projectPath === null ? "user" : "project",
      projectPath,
      dest: `${projectPath ?? "~"}/${harness}/${slug}.md`,
    }),
  ];
}

/**
 * Answer the IPC the component actually issues. `installs_reconcile` comes from
 * `projects.refresh()` on mount; an unexpected command fails loudly instead of
 * silently returning undefined.
 */
function serve(options: Partial<Backend> = {}): Backend {
  const state: Backend = {
    ledger: options.ledger ?? [],
    preview: options.preview ?? ((slug, projectPath) => previewData(slug, projectPath)),
    install: options.install,
    uninstall: options.uninstall,
  };
  // The component derives every cell from the store, not from an IPC call, so
  // the seeded ledger is the store's install truth too.
  rules.installations = state.ledger;
  mocks.invoke.mockImplementation(async (command: string, args: Record<string, unknown> = {}) => {
    if (command === "installs_reconcile") return [];
    if (command === "rules_preview") {
      return state.preview(String(args.slug), (args.projectPath as string | null) ?? null);
    }
    const slug = String(args.slug);
    const harness = String(args.harness);
    const projectPath = (args.projectPath as string | null) ?? null;
    if (command === "rules_install") {
      if (state.install !== undefined) state.install(slug, harness, projectPath);
      else addInstall(state, slug, harness, projectPath);
      return ruleInstall({
        slug,
        tool: harness,
        scope: projectPath === null ? "user" : "project",
        projectPath,
      });
    }
    if (command === "rules_uninstall") {
      if (state.uninstall !== undefined) state.uninstall(slug, harness, projectPath);
      else
        state.ledger = state.ledger.filter(
          (row) =>
            !(
              row.slug === slug &&
              row.tool === harness &&
              (row.projectPath ?? null) === projectPath
            ),
        );
      return null;
    }
    if (command === "rules_installs") return state.ledger;
    throw new Error(`unexpected command ${command}`);
  });
  return state;
}

function mount(ruleSlugs: string[], onClose: () => void = vi.fn()) {
  return render(InstallModal, {
    props: {
      title: "Install",
      target: { kind: "rules" as const, ruleSlugs },
      onClose,
    },
  });
}

function toggleButtons(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>(".grid button.toggle"));
}

/** The destination cell that starts one row, located by its rendered text. */
function rowElement(projectPath: string | null): HTMLElement {
  const destCells = Array.from(document.querySelectorAll<HTMLElement>(".grid .cell.dest"));
  const match = destCells.find((cell) => {
    const path = cell.querySelector<HTMLElement>(".d-path");
    return projectPath === null
      ? path?.textContent === i18n.t("common.everyMachine")
      : path?.getAttribute("title") === projectPath;
  });
  if (match === undefined) throw new Error(`no row for ${projectPath}`);
  return match;
}

/** Every toggle in one row, in column order. */
function cellsInRow(projectPath: string | null): HTMLElement[] {
  const cells: HTMLElement[] = [];
  let node = rowElement(projectPath).nextElementSibling as HTMLElement | null;
  while (node !== null && !node.classList.contains("dest")) {
    if (node.matches("button.toggle")) cells.push(node);
    node = node.nextElementSibling as HTMLElement | null;
  }
  return cells;
}

/** The cell for one harness in one row (rule mode renders every column). */
function cellFor(harnessId: string, projectPath: string | null): HTMLElement {
  const index = installableHarnesses().findIndex((harness) => harness.id === harnessId);
  if (index < 0) throw new Error(`unknown harness ${harnessId}`);
  const cell = cellsInRow(projectPath)[index];
  if (cell === undefined) throw new Error(`no cell for ${harnessId} at ${projectPath}`);
  return cell;
}

function previewCalls(): unknown[][] {
  return mocks.invoke.mock.calls.filter(([command]) => command === "rules_preview");
}

beforeEach(() => {
  mocks.invoke.mockReset();
  rules.list = [ruleSummary()];
  rules.installations = [];
  rules.busy = null;
  rules.previewResult = null;
  rules.previewError = null;
  rules.previewLoading = false;
  install.installed = [];
  toast.items = [];
  // Seed the project row through the store (D10): `projects.hydrated` is a
  // module-level flag, so localStorage seeding works only in the first test.
  projects.register("/A");
});

describe("rule mode — cell state comes from the ledger", () => {
  it("(a) marks every cell of a fully installed row as pressed", async () => {
    serve({
      ledger: installableHarnesses().map((harness) =>
        ruleInstall({
          slug: "all-on-rule",
          tool: harness.id,
          scope: "project",
          projectPath: "/A",
          dest: `/A/${harness.id}/all-on-rule.md`,
        }),
      ),
    });

    mount(["all-on-rule"]);

    await waitFor(() => expect(toggleButtons()).toHaveLength(8));
    for (const harness of installableHarnesses()) {
      const cell = cellFor(harness.id, "/A");
      expect(cell).toBeEnabled();
      expect(cell.getAttribute("aria-pressed")).toBe("true");
      expect(cell.getAttribute("aria-label")).toBe(
        i18n.t("rules.install.cellOn", {
          harness: harness.label,
          target: "A",
          dest: `/A/${harness.id}/all-on-rule.md`,
        }),
      );
      // The same harness on the Global row has no ledger row.
      expect(cellFor(harness.id, null).getAttribute("aria-pressed")).toBe("false");
    }
  });

  it("(b) leaves every cell off when nothing is installed", async () => {
    serve();

    mount(["none-on-rule"]);

    await waitFor(() => expect(toggleButtons()).toHaveLength(8));
    for (const harness of installableHarnesses()) {
      await waitFor(() => expect(cellFor(harness.id, null)).toBeEnabled());
      const cell = cellFor(harness.id, null);
      expect(cell.getAttribute("aria-pressed")).toBe("false");
      expect(cell.getAttribute("aria-label")).toBe(
        i18n.t("rules.install.cellOff", {
          harness: harness.label,
          target: i18n.t("common.global"),
          dest: `~/${harness.id}/none-on-rule.md`,
        }),
      );
    }
  });

  it("(c) labels each slug's cells with that slug's own destination in one row", async () => {
    const [first, second, third, fourth] = installableHarnesses();
    serve({
      ledger: [
        ruleInstall({
          slug: "per-slug-a",
          tool: first.id,
          scope: "user",
          projectPath: null,
          dest: `~/${first.id}/per-slug-a.md`,
        }),
      ],
      // Disjoint payloads: the two slugs own different cells of the SAME row.
      preview: (slug, projectPath) =>
        slug === "per-slug-a"
          ? previewHalf(slug, projectPath, [first.id, second.id])
          : previewHalf(slug, projectPath, [third.id, fourth.id]),
    });

    mount(["per-slug-a", "per-slug-b"]);

    await waitFor(() => expect(toggleButtons()).toHaveLength(8));
    await waitFor(() => {
      // Slug A answered the first two columns; slug B the last two. A single
      // row cache would leave the loser's columns with no destination ("—").
      const partial = cellFor(first.id, null);
      expect(partial.getAttribute("aria-pressed")).toBe("false");
      expect(partial.className).toContain("partial");
      expect(partial).toBeEnabled();
      expect(partial.getAttribute("aria-label")).toBe(
        i18n.t("rules.install.cellPartial", {
          harness: first.label,
          target: i18n.t("common.global"),
          dest: `~/${first.id}/per-slug-a.md`,
        }),
      );
      expect(cellFor(second.id, null).getAttribute("aria-label")).toBe(
        i18n.t("rules.install.cellOff", {
          harness: second.label,
          target: i18n.t("common.global"),
          dest: `~/${second.id}/per-slug-a.md`,
        }),
      );
      expect(cellFor(third.id, null)).toBeEnabled();
      expect(cellFor(third.id, null).getAttribute("aria-label")).toBe(
        i18n.t("rules.install.cellOff", {
          harness: third.label,
          target: i18n.t("common.global"),
          dest: `~/${third.id}/per-slug-b.md`,
        }),
      );
      expect(cellFor(fourth.id, null).getAttribute("aria-label")).toBe(
        i18n.t("rules.install.cellOff", {
          harness: fourth.label,
          target: i18n.t("common.global"),
          dest: `~/${fourth.id}/per-slug-b.md`,
        }),
      );
    });
    // One preview per (row, slug): two rows × two slugs.
    expect(previewCalls()).toHaveLength(4);
  });

  it("(c2) does not settle a row as loaded when one slug of the payload rejects", async () => {
    const harness = installableHarnesses()[0];
    serve({
      preview: (slug, projectPath) => {
        if (slug === "reject-b") throw new Error("compile failed");
        return previewData(slug, projectPath);
      },
    });

    mount(["reject-a", "reject-b"]);

    await waitFor(() => expect(toggleButtons()).toHaveLength(8));
    // The surviving slug still supplies a destination, but the row's error gates
    // every cell: nothing here reads as resolved-and-ready.
    await waitFor(() => {
      for (const column of installableHarnesses()) {
        const cell = cellFor(column.id, null);
        expect(cell).toBeDisabled();
        expect(cell.getAttribute("title")).toBe(i18n.t("rules.install.reasonPreviewFailed"));
        expect(cell.getAttribute("aria-label")).toContain(
          i18n.t("rules.install.reasonPreviewFailed"),
        );
      }
    });
    expect(cellFor(harness.id, null).getAttribute("aria-label")).toContain(
      `~/${harness.id}/reject-a.md`,
    );
    expect(previewCalls()).toHaveLength(4);
  });
});

describe("rule mode — columns and destinations", () => {
  it("(d) offers exactly the four installable harnesses and no agent tool", async () => {
    serve();

    mount(["columns-rule"]);

    await waitFor(() => expect(document.querySelectorAll(".head.tool")).toHaveLength(4));
    const labels = Array.from(document.querySelectorAll(".head.tool")).map((node) =>
      node.textContent?.trim(),
    );
    expect(labels).toEqual(installableHarnesses().map((harness) => harness.label));
    expect(screen.queryByText("Claude Code")).toBeNull();
    expect(screen.queryByText("Cursor")).toBeNull();
  });

  it("(e) disables install with the preview reason but keeps an install removable", async () => {
    const installed = installableHarnesses()[0];
    serve({
      ledger: [
        ruleInstall({
          slug: "preview-fail-rule",
          tool: installed.id,
          scope: "project",
          projectPath: "/A",
          dest: "/A/installed-dest.md",
        }),
      ],
      preview: () => {
        throw new Error("compile failed");
      },
    });

    mount(["preview-fail-rule"]);

    await waitFor(() => expect(toggleButtons()).toHaveLength(8));
    // D1: the reason gates INSTALL. An already-installed cell stays clickable
    // so its ledger row can still be removed.
    await waitFor(() => {
      for (const harness of installableHarnesses()) {
        const cell = cellFor(harness.id, "/A");
        if (harness.id === installed.id) {
          expect(cell).toBeEnabled();
          continue;
        }
        expect(cell).toBeDisabled();
        expect(cell.getAttribute("title")).toBe(i18n.t("rules.install.reasonPreviewFailed"));
        expect(cell.getAttribute("aria-label")).toContain(
          i18n.t("rules.install.reasonPreviewFailed"),
        );
      }
    });

    await fireEvent.click(cellFor(installed.id, "/A"));
    expect(
      await screen.findByText(i18n.t("rules.install.gridRemoveTitle", { count: 1 })),
    ).toBeInTheDocument();
  });

  it("(g) compiles one preview per row, not one per cell", async () => {
    serve();

    mount(["once-rule"]);

    await waitFor(() => expect(toggleButtons()).toHaveLength(8));
    await waitFor(() => expect(previewCalls()).toHaveLength(2));
    // A re-render must not fan the previews out again.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(previewCalls()).toHaveLength(2);
  });
});

describe("rule mode — live region", () => {
  it("(h) mounts the status region before any action and keeps that same node", async () => {
    serve();

    mount(["live-region-rule"]);

    await waitFor(() => expect(toggleButtons()).toHaveLength(8));
    const status = screen.getByRole("status");
    expect(status.getAttribute("aria-live")).toBe("polite");
    // Created empty: a live region inserted together with its text is not
    // reliably announced, so the node exists before there is anything to say.
    expect(status.textContent?.trim()).toBe("");

    const harness = installableHarnesses()[0];
    await waitFor(() => expect(cellFor(harness.id, null)).toBeEnabled());
    await fireEvent.click(cellFor(harness.id, null));
    await fireEvent.click(
      await screen.findByRole("button", { name: i18n.t("rules.install.action") }),
    );

    await waitFor(() => expect(status.textContent?.trim()).not.toBe(""));
    // Same element, not a re-created one: the condition lives INSIDE the region.
    expect(screen.getByRole("status")).toBe(status);
  });
});

describe("rule mode — one action, one toast", () => {
  it("(f) reports a partial failure as one error toast and never as success", async () => {
    let backend: Backend;
    backend = serve({
      install: (slug, harness, projectPath) => {
        if (slug === "partial-b") throw new Error("disk full");
        addInstall(backend, slug, harness, projectPath);
      },
    });

    mount(["partial-a", "partial-b"]);

    const harness = installableHarnesses()[0];
    await waitFor(() => expect(cellFor(harness.id, null)).toBeEnabled());
    await fireEvent.click(cellFor(harness.id, null));

    expect(
      await screen.findByText(i18n.t("rules.install.gridInstallTitle", { count: 2 })),
    ).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: i18n.t("rules.install.action") }));

    await waitFor(() => expect(toast.items).toHaveLength(1));
    expect(toast.items[0]).toMatchObject({
      kind: "error",
      title: i18n.t("rules.install.gridPartial", { ok: 1, fail: 1 }),
    });

    // The cell that received one of two slugs is PARTIAL — never announced as on.
    await waitFor(() => {
      const cell = cellFor(harness.id, null);
      expect(cell.getAttribute("aria-pressed")).toBe("false");
      expect(cell.className).toContain("partial");
    });
    expect(backend.ledger).toHaveLength(1);

    // A sibling cell was not touched by this action.
    const other = installableHarnesses()[1];
    expect(cellFor(other.id, null).getAttribute("aria-pressed")).toBe("false");
    expect(cellFor(other.id, null).className).not.toContain("partial");
  });
});

describe("rule mode — dialogs stack instead of colliding", () => {
  it("(i) Escape closes only the confirm and keeps the grid and its previews mounted", async () => {
    serve();

    mount(["stack-rule"]);

    const harness = installableHarnesses()[0];
    await waitFor(() => expect(cellFor(harness.id, null)).toBeEnabled());
    await fireEvent.click(cellFor(harness.id, null));

    const installTitle = i18n.t("rules.install.gridInstallTitle", { count: 1 });
    expect(await screen.findByRole("dialog", { name: installTitle })).toBeInTheDocument();

    await fireEvent.keyDown(window, { key: "Escape" });

    // The confirm is gone…
    await waitFor(() => expect(screen.queryByRole("dialog", { name: installTitle })).toBeNull());
    // …and the grid under it never saw the Escape: still mounted, still populated.
    expect(screen.getByRole("dialog", { name: "Install" })).toBeInTheDocument();
    expect(toggleButtons()).toHaveLength(8);
  });

  it("(j) Escape still closes the grid when it is the only dialog open", async () => {
    serve();
    // InstallModal is unconditionally open — its parent owns visibility — so the
    // only observable close is the callback. This also proves the stack left no
    // residue behind after the previous test unmounted.
    const onClose = vi.fn();

    mount(["solo-rule"], onClose);

    await waitFor(() => expect(toggleButtons()).toHaveLength(8));
    expect(screen.getByRole("dialog", { name: "Install" })).toBeInTheDocument();

    await fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it("(k) names the confirm with its own title, not the grid's", async () => {
    const harness = installableHarnesses()[0];
    serve({
      ledger: [
        ruleInstall({
          slug: "named-rule",
          tool: harness.id,
          scope: "user",
          projectPath: null,
          dest: `~/${harness.id}/named-rule.md`,
        }),
      ],
    });

    mount(["named-rule"]);

    await waitFor(() => expect(cellFor(harness.id, null).getAttribute("aria-pressed")).toBe("true"));
    await fireEvent.click(cellFor(harness.id, null));

    // Located by accessible name: with a shared id this resolved to "Install".
    const removeTitle = i18n.t("rules.install.gridRemoveTitle", { count: 1 });
    expect(await screen.findByRole("dialog", { name: removeTitle })).toBeInTheDocument();
    // Both dialogs keep a distinct name, so each id resolves to its own heading.
    expect(screen.getByRole("dialog", { name: "Install" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: removeTitle })).toBeInTheDocument();
  });
});

describe("rule mode — zero projects", () => {
  it("keeps the Add project affordance and explains why", async () => {
    projects.unregister("/A");
    serve();

    mount(["no-project-rule"]);

    await waitFor(() => expect(toggleButtons()).toHaveLength(4));
    expect(screen.getByText(i18n.t("rules.install.gridProjectHint"))).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: new RegExp(i18n.t("install.addProject")) }),
    ).toBeInTheDocument();
  });
});
