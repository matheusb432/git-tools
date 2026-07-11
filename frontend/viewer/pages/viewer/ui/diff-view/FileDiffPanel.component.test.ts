import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import type { ApiTransport, RowsPage } from "@/shared/api";
import { renderWithData } from "@/shared/test";
import { TEST_IDS } from "@/shared/testids";
import FileDiffPanelTestHarness from "./FileDiffPanelTestHarness.svelte";

const mountedTest = process.env.VITEST === "true" ? test : test.skip;

mountedTest("same-range density transition replaces compact rows with full rows", async () => {
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      if (operation !== "file_rows") throw new Error(`Unexpected operation: ${operation}`);
      const layout = input.layout;
      const density = input.full === true ? "full" : "compact";
      if (layout !== "unified") return { total: 0, layout: "split", rows: [] } satisfies RowsPage;
      return {
        total: 2,
        layout: "unified",
        rows: [
          {
            kind: "add",
            old_no: null,
            new_no: 1,
            text: `${layout}:${density}`,
            owner: null,
            long_len: null,
          },
          {
            kind: "context",
            old_no: 2,
            new_no: 2,
            text: "unchanged range sentinel",
            owner: null,
            long_len: null,
          },
        ],
      } satisfies RowsPage;
    },
    listen: async () => () => undefined,
  };
  const { rerender } = renderWithData(FileDiffPanelTestHarness, { full: false }, transport);

  expect(await screen.findByText("+unified:compact")).toBeVisible();
  await rerender({ full: true });

  expect(await screen.findByText("+unified:full")).toBeVisible();
  expect(screen.queryByText("+unified:compact")).not.toBeInTheDocument();
});

mountedTest("a visible zero-estimate panel bootstraps page zero", async () => {
  const transport: ApiTransport = {
    invoke: async () => ({
      total: 1,
      layout: "unified",
      rows: [{ kind: "add", old_no: null, new_no: 1, text: "bootstrapped", owner: null, long_len: null }],
    }),
    listen: async () => () => undefined,
  };

  renderWithData(FileDiffPanelTestHarness, { full: false, added: 0, removed: 0 }, transport);

  expect(await screen.findByText("+bootstrapped")).toBeVisible();
});

mountedTest("a failed row observer refetches through Query", async () => {
  let attempts = 0;
  const transport: ApiTransport = {
    invoke: async (_operation, input = {}) => {
      if (input.layout === "split") return { total: 0, layout: "split", rows: [] };
      attempts += 1;
      if (attempts === 1) throw new Error("row page unavailable");
      return {
        total: 1,
        layout: "unified",
        rows: [{ kind: "add", old_no: null, new_no: 1, text: "retried", owner: null, long_len: null }],
      };
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  renderWithData(FileDiffPanelTestHarness, { full: false }, transport);

  expect(await screen.findByText("row page unavailable")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Retry" }));

  expect(await screen.findByText("+retried")).toBeVisible();
  expect(screen.queryByText("row page unavailable")).not.toBeInTheDocument();
});

mountedTest("collapsed and offscreen panels create no row observers", async () => {
  const operations: string[] = [];
  const transport: ApiTransport = {
    invoke: async (operation) => {
      operations.push(operation);
      return { total: 0, layout: "unified", rows: [] };
    },
    listen: async () => () => undefined,
  };
  const { queryClient, rerender } = renderWithData(
    FileDiffPanelTestHarness,
    { full: false, expanded: false },
    transport,
  );

  await waitFor(() =>
    expect(
      queryClient
        .getQueryCache()
        .getAll()
        .flatMap((query) => query.observers),
    ).toHaveLength(0),
  );
  await rerender({
    full: false,
    expanded: true,
    panelTop: 10_000,
    panelHeight: 96,
    outerScrollTop: 0,
    outerViewportHeight: 400,
  });
  await waitFor(() =>
    expect(
      queryClient
        .getQueryCache()
        .getAll()
        .flatMap((query) => query.observers),
    ).toHaveLength(0),
  );
  expect(operations).toEqual([]);
});

mountedTest("a tall visible row window renders its last visible row with two observers", async () => {
  const transport: ApiTransport = {
    invoke: async (_operation, input = {}) => {
      const start = typeof input.start === "number" ? input.start : 0;
      const count = typeof input.count === "number" ? input.count : 0;
      return {
        total: 500,
        layout: "unified",
        rows: Array.from({ length: Math.min(count, 500 - start) }, (_, offset) => ({
          kind: "context" as const,
          old_no: start + offset + 1,
          new_no: start + offset + 1,
          text: `row-${start + offset}`,
          owner: null,
          long_len: null,
        })),
      } satisfies RowsPage;
    },
    listen: async () => () => undefined,
  };
  const { queryClient } = renderWithData(
    FileDiffPanelTestHarness,
    {
      full: false,
      added: 500,
      panelHeight: 12_000,
      outerScrollTop: 4_000,
      outerViewportHeight: 400,
    },
    transport,
  );

  await waitFor(() => {
    const observed = queryClient
      .getQueryCache()
      .getAll()
      .filter((query) => query.observers.length > 0);
    expect(observed).toHaveLength(2);
    expect(observed.map((query) => query.queryKey.at(-2))).toEqual([0, 80]);
    expect(observed.map((query) => query.queryKey.at(-1))).toEqual([80, 160]);
  });
  expect(await screen.findByText("row-170")).toBeVisible();
});

mountedTest("a combined pending row span covers the last aligned page until it resolves", async () => {
  let resolveCombinedPage: ((page: RowsPage) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (_operation, input = {}) => {
      const start = typeof input.start === "number" ? input.start : 0;
      const count = typeof input.count === "number" ? input.count : 0;
      if (start === 80) {
        return new Promise<RowsPage>((resolve) => {
          resolveCombinedPage = resolve;
        });
      }
      return {
        total: 500,
        layout: "unified",
        rows: Array.from({ length: Math.min(count, 500 - start) }, (_, offset) => ({
          kind: "context" as const,
          old_no: start + offset + 1,
          new_no: start + offset + 1,
          text: `row-${start + offset}`,
          owner: null,
          long_len: null,
        })),
      } satisfies RowsPage;
    },
    listen: async () => () => undefined,
  };
  renderWithData(
    FileDiffPanelTestHarness,
    {
      full: false,
      added: 500,
      panelHeight: 12_000,
      outerScrollTop: 4_000,
      outerViewportHeight: 400,
    },
    transport,
  );

  await waitFor(() => expect(resolveCombinedPage).toBeTypeOf("function"));
  const lastVisibleRow = screen.getByTestId(TEST_IDS.diffView.rowWindow).lastElementChild;
  if (!(lastVisibleRow instanceof HTMLElement)) throw new Error("The row window has no last row");
  expect(lastVisibleRow).toHaveAttribute("data-testid", TEST_IDS.diffView.rowSkeleton);

  resolveCombinedPage?.({
    total: 500,
    layout: "unified",
    rows: Array.from({ length: 160 }, (_, offset) => ({
      kind: "context" as const,
      old_no: 81 + offset,
      new_no: 81 + offset,
      text: `row-${80 + offset}`,
      owner: null,
      long_len: null,
    })),
  });

  expect(await screen.findByText("row-170")).toBeVisible();
  expect(screen.queryAllByTestId(TEST_IDS.diffView.rowSkeleton)).toHaveLength(0);
});

mountedTest("pending page zero renders skeletons before an empty result", async () => {
  let resolvePage: ((page: RowsPage) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async () =>
      new Promise<RowsPage>((resolve) => {
        resolvePage = resolve;
      }),
    listen: async () => () => undefined,
  };
  renderWithData(FileDiffPanelTestHarness, { full: false }, transport);

  await waitFor(() => expect(resolvePage).toBeTypeOf("function"));
  expect(screen.queryAllByTestId(TEST_IDS.diffView.rowSkeleton)).not.toHaveLength(0);
  expect(screen.queryByText("No diff rows are available for this file.")).not.toBeInTheDocument();

  resolvePage?.({ total: 0, layout: "unified", rows: [] });

  expect(await screen.findByText("No diff rows are available for this file.")).toBeVisible();
  expect(screen.queryAllByTestId(TEST_IDS.diffView.rowSkeleton)).toHaveLength(0);
});
