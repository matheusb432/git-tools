import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import type { ApiTransport } from "@/shared/api";
import { recipe, renderWithData, tabMeta } from "@/shared/test";
import { pendingRecipeBatchesKey, tabMetaKey } from "../api/recipe-tabs.svelte";
import ViewerPage from "./ViewerPage.svelte";

const mountedTest = process.env.VITEST === "true" ? test : test.skip;

type TransportOperation = {
  readonly operation: string;
  readonly input: Record<string, unknown>;
};

function pendingBatch() {
  return [{ batch_id: "batch-1", recipes: [recipe] }];
}

function reviewStoreCount(): number | undefined {
  const bridge: unknown = Reflect.get(window, "__GTL_VIEWER_TEST__");
  if (typeof bridge !== "object" || bridge === null || !("snapshot" in bridge)) return undefined;
  const snapshot = bridge.snapshot;
  if (typeof snapshot !== "function") return undefined;
  const value: unknown = snapshot();
  if (typeof value !== "object" || value === null || !("reviewStoreCount" in value)) return undefined;
  return typeof value.reviewStoreCount === "number" ? value.reviewStoreCount : undefined;
}

function rowPageQueryCount(): number | undefined {
  const bridge: unknown = Reflect.get(window, "__GTL_VIEWER_TEST__");
  if (typeof bridge !== "object" || bridge === null || !("snapshot" in bridge)) return undefined;
  const snapshot = bridge.snapshot;
  if (typeof snapshot !== "function") return undefined;
  const value: unknown = snapshot();
  if (typeof value !== "object" || value === null || !("queryCache" in value)) return undefined;
  const queryCache: unknown = value.queryCache;
  if (typeof queryCache !== "object" || queryCache === null || !("rowPages" in queryCache)) return undefined;
  return typeof queryCache.rowPages === "number" ? queryCache.rowPages : undefined;
}

mountedTest("the viewer test snapshot counts only row-page queries", async () => {
  const transport: ApiTransport = {
    invoke: async (operation) => {
      if (operation === "drain_pending_recipes") return [];
      if (operation === "list_live_views") return [];
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const { queryClient } = renderWithData(ViewerPage, {}, transport);

  queryClient.setQueryData(["viewer", "tab", 7, "file", 3, "rows", "split", "full", 0, 80], { rows: [] });
  queryClient.setQueryData(["viewer", "tab", 7, "meta"], { title: "unrelated" });
  queryClient.setQueryData(["viewer", "tab", 7, "file", 3, "rows-extra"], { rows: [] });

  await waitFor(() => expect(rowPageQueryCount()).toBe(1));
});

mountedTest("a late open closes the orphaned backend tab and removes its seeded cache", async () => {
  const operations: TransportOperation[] = [];
  let resolveOpen: ((value: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "drain_pending_recipes") return pendingBatch();
      if (operation === "list_live_views") return [];
      if (operation === "open_recipe") {
        return new Promise<unknown>((resolve) => {
          resolveOpen = resolve;
        });
      }
      if (operation === "close_tab") return true;
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(ViewerPage, {}, transport);
  const close = await screen.findByRole("button", { name: "Close Opening diff" });
  expect(
    queryClient.getQueryCache().find({ queryKey: pendingRecipeBatchesKey, exact: true })?.observers[0]?.options
      .staleTime,
  ).toBe(Infinity);

  await user.click(close);
  resolveOpen?.({ tab_id: 7, meta: tabMeta });

  await waitFor(() => expect(operations.some((entry) => entry.operation === "close_tab")).toBe(true));
  expect(screen.queryByRole("tab")).not.toBeInTheDocument();
  expect(queryClient.getQueryData(tabMetaKey(7))).toBeUndefined();
});

mountedTest("a closed late open does not destroy a reused backend tab still owned by a survivor", async () => {
  const operations: TransportOperation[] = [];
  let openCount = 0;
  let resolveReusedOpen: ((value: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "drain_pending_recipes") {
        return [{ batch_id: "batch-reuse", recipes: [recipe, recipe] }];
      }
      if (operation === "list_live_views") return [];
      if (operation === "open_recipe") {
        openCount += 1;
        if (openCount === 1) return { tab_id: 9, meta: { ...tabMeta, tab_id: 9 } };
        return new Promise<unknown>((resolve) => {
          resolveReusedOpen = resolve;
        });
      }
      if (operation === "get_setting") return null;
      if (operation === "close_tab") return true;
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(ViewerPage, {}, transport);
  await waitFor(() => expect(typeof resolveReusedOpen).toBe("function"));

  await user.click(screen.getByRole("button", { name: "Close Opening diff" }));
  resolveReusedOpen?.({ tab_id: 9, meta: { ...tabMeta, tab_id: 9, batch_id: "batch-reuse" } });

  await waitFor(() =>
    expect(
      queryClient
        .getMutationCache()
        .getAll()
        .filter(
          (item) =>
            item.state.status === "success" &&
            item.options.mutationKey?.[0] === "viewer" &&
            item.options.mutationKey[1] === "tab" &&
            item.options.mutationKey[2] === "open",
        ),
    ).toHaveLength(2),
  );
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(screen.getAllByRole("tab")).toHaveLength(1);
  expect(screen.getByRole("tab")).toHaveAttribute("aria-selected", "true");
  expect(await screen.findByRole("button", { name: "Refresh" })).toBeVisible();
  expect(queryClient.getQueryData(tabMetaKey(9))).toEqual({ ...tabMeta, tab_id: 9, batch_id: "batch-reuse" });
  expect(operations.filter((entry) => entry.operation === "close_tab")).toEqual([]);
});

mountedTest("close drops the shell, tab queries, and review store before backend close settles", async () => {
  const operations: TransportOperation[] = [];
  let resolveClose: ((value: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "drain_pending_recipes") return pendingBatch();
      if (operation === "list_live_views") return [];
      if (operation === "open_recipe") return { tab_id: 7, meta: tabMeta };
      if (operation === "get_setting") return null;
      if (operation === "close_tab") {
        return new Promise<unknown>((resolve) => {
          resolveClose = resolve;
        });
      }
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(ViewerPage, {}, transport);
  const close = await screen.findByRole("button", { name: "Close repo diff" });
  await waitFor(() => expect(reviewStoreCount()).toBe(1));

  await user.click(close);

  await waitFor(() => expect(operations.some((entry) => entry.operation === "close_tab")).toBe(true));
  expect(screen.queryByRole("tab")).not.toBeInTheDocument();
  expect(queryClient.getQueryData(tabMetaKey(7))).toBeUndefined();
  expect(reviewStoreCount()).toBe(0);
  expect(
    queryClient
      .getMutationCache()
      .getAll()
      .map((mutation) => mutation.options.mutationKey),
  ).toContainEqual(["viewer", "tab", 7, "close"]);
  resolveClose?.(true);
});

mountedTest("late refresh success cannot resurrect cache after its originating shell closes", async () => {
  let resolveRefresh: ((value: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (operation) => {
      if (operation === "drain_pending_recipes") return pendingBatch();
      if (operation === "list_live_views") return [];
      if (operation === "open_recipe") return { tab_id: 7, meta: tabMeta };
      if (operation === "get_setting") return null;
      if (operation === "refresh_tab") {
        return new Promise<unknown>((resolve) => {
          resolveRefresh = resolve;
        });
      }
      if (operation === "close_tab") return true;
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(ViewerPage, {}, transport);
  await user.click(await screen.findByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(typeof resolveRefresh).toBe("function"));

  await user.click(screen.getByRole("button", { name: "Close repo diff" }));
  await waitFor(() => expect(screen.queryByRole("tab")).not.toBeInTheDocument());
  expect(queryClient.getQueryData(tabMetaKey(7))).toBeUndefined();
  expect(reviewStoreCount()).toBe(0);

  resolveRefresh?.({ ...tabMeta, batch_id: "batch-refreshed" });
  await waitFor(() =>
    expect(
      queryClient
        .getMutationCache()
        .getAll()
        .some(
          (item) =>
            item.state.status === "success" &&
            item.options.mutationKey?.[0] === "viewer" &&
            item.options.mutationKey[1] === "tab" &&
            item.options.mutationKey[2] === 7 &&
            item.options.mutationKey[3] === "refresh",
        ),
    ).toBe(true),
  );

  expect(queryClient.getQueryData(tabMetaKey(7))).toBeUndefined();
  expect(screen.queryByRole("tab")).not.toBeInTheDocument();
  expect(reviewStoreCount()).toBe(0);
});

mountedTest("delayed listener registration stops immediately after the page has unmounted", async () => {
  let resolveListen: ((stop: () => void) => void) | undefined;
  let stopped = false;
  const transport: ApiTransport = {
    invoke: async (operation) => {
      if (operation === "drain_pending_recipes" || operation === "list_live_views") return [];
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () =>
      new Promise<() => void>((resolve) => {
        resolveListen = resolve;
      }),
  };
  const rendered = renderWithData(ViewerPage, {}, transport);

  rendered.unmount();
  resolveListen?.(() => {
    stopped = true;
  });

  await waitFor(() => expect(stopped).toBe(true));
});

mountedTest("pending drain and event batches share one sequential open queue", async () => {
  const operations: TransportOperation[] = [];
  const openResolvers: Array<(value: unknown) => void> = [];
  let receiveRecipe: ((payload: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "drain_pending_recipes") return pendingBatch();
      if (operation === "list_live_views") return [];
      if (operation === "open_recipe") {
        return new Promise<unknown>((resolve) => openResolvers.push(resolve));
      }
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async (_event, receive) => {
      receiveRecipe = receive;
      return () => undefined;
    },
  };
  renderWithData(ViewerPage, {}, transport);
  await waitFor(() => expect(operations.filter((entry) => entry.operation === "open_recipe")).toHaveLength(1));
  await waitFor(() => expect(typeof receiveRecipe).toBe("function"));

  receiveRecipe?.({ batch_id: "batch-2", recipes: [{ ...recipe, source: { ...recipe.source, value: "/repo-2" } }] });
  await Promise.resolve();

  expect(operations.filter((entry) => entry.operation === "open_recipe")).toHaveLength(1);
  openResolvers[0]?.({ tab_id: 7, meta: tabMeta });
  await waitFor(() => expect(operations.filter((entry) => entry.operation === "open_recipe")).toHaveLength(2));
  expect(operations.filter((entry) => entry.operation === "open_recipe").map((entry) => entry.input.batchId)).toEqual([
    "batch-1",
    "batch-2",
  ]);
  openResolvers[1]?.({ tab_id: 8, meta: { ...tabMeta, tab_id: 8, batch_id: "batch-2" } });
});
