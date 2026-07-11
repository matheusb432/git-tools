import { QueryClient } from "@tanstack/svelte-query";
import { expect, test } from "bun:test";
import { createApiClient, type ApiTransport } from "@/shared/api";
import { resetTabRowPages, rowPageKey, rowPageOptions } from "./row-pages.svelte";

const base = {
  tabId: 7,
  fileIdx: 1,
  layout: "unified" as const,
  density: "compact" as const,
  start: 0,
  count: 80,
};

test("row key includes complete pane and page identity", () => {
  expect(rowPageKey(base)).toEqual(["viewer", "tab", 7, "file", 1, "rows", "unified", "compact", 0, 80]);
  expect(rowPageKey({ ...base, density: "full" })).not.toEqual(rowPageKey(base));
  expect(rowPageKey({ ...base, layout: "split" })).not.toEqual(rowPageKey(base));
  expect(rowPageKey({ ...base, tabId: 8 })).not.toEqual(rowPageKey(base));
});

test("row options query the generic API with pane identity and an abort signal", async () => {
  const calls: Array<{
    readonly operation: string;
    readonly input: Record<string, unknown>;
    readonly aborted: boolean;
  }> = [];
  const transport: ApiTransport = {
    invoke: async (operation, input = {}, signal) => {
      calls.push({ operation, input, aborted: signal?.aborted ?? false });
      return { total: 0, layout: "unified", rows: [] };
    },
    listen: async () => () => undefined,
  };
  const client = new QueryClient();

  await client.fetchQuery(rowPageOptions(createApiClient(transport), base));

  expect(calls).toEqual([
    {
      operation: "file_rows",
      input: { tabId: 7, fileIdx: 1, layout: "unified", full: false, start: 0, count: 80 },
      aborted: false,
    },
  ]);
  expect(rowPageOptions(createApiClient(transport), base).staleTime).toBe(Infinity);
  expect(rowPageOptions(createApiClient(transport), base).gcTime).toBe(30_000);
});

test("tab row reset preserves metadata and clears every pane page", async () => {
  const client = new QueryClient();
  client.setQueryData(["viewer", "tab", 7, "meta"], { title: "kept" });
  client.setQueryData(rowPageKey(base), { total: 0, layout: "unified", rows: [] });
  client.setQueryData(rowPageKey({ ...base, layout: "split" }), { total: 0, layout: "split", rows: [] });

  await resetTabRowPages(client, 7);

  expect(client.getQueryData<{ readonly title: string }>(["viewer", "tab", 7, "meta"])).toEqual({ title: "kept" });
  expect(client.getQueryData(rowPageKey(base))).toBeUndefined();
  expect(client.getQueryData(rowPageKey({ ...base, layout: "split" }))).toBeUndefined();
});
