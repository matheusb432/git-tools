import { expect, test } from "bun:test";
import { QueryClient } from "@tanstack/svelte-query";
import { pendingRecipeBatchesKey, removeTabQueries, tabMetaKey, tabPrefix } from "./recipe-tabs.svelte";

test("tab resource keys are hierarchical and transport neutral", () => {
  expect(pendingRecipeBatchesKey).toEqual(["viewer", "pending-recipe-batches"]);
  expect(tabPrefix(7)).toEqual(["viewer", "tab", 7]);
  expect(tabMetaKey(7)).toEqual(["viewer", "tab", 7, "meta"]);
});

test("removing a tab cancels and removes only resources under that tab prefix", async () => {
  const queryClient = new QueryClient();
  let cancelled = false;
  const pending = queryClient.fetchQuery({
    queryKey: [...tabPrefix(7), "rows"],
    queryFn: ({ signal }) =>
      new Promise<never>((_resolve, reject) => {
        signal.addEventListener("abort", () => {
          cancelled = true;
          reject(signal.reason);
        });
      }),
  });
  queryClient.setQueryData(tabMetaKey(7), { tab: 7 });
  queryClient.setQueryData(tabMetaKey(8), { tab: 8 });
  queryClient.setQueryData(pendingRecipeBatchesKey, []);

  await removeTabQueries(queryClient, 7);
  await pending.catch(() => undefined);

  expect(cancelled).toBe(true);
  expect(queryClient.getQueriesData({ queryKey: tabPrefix(7) })).toEqual([]);
  expect(queryClient.getQueryData<{ readonly tab: number }>(tabMetaKey(8))).toEqual({ tab: 8 });
  expect(queryClient.getQueryData<readonly unknown[]>(pendingRecipeBatchesKey)).toEqual([]);
});
