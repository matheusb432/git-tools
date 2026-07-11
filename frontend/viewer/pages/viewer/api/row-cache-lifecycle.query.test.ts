import { QueryClient } from "@tanstack/svelte-query";
import { afterEach, expect, test, vi } from "vitest";

afterEach(() => vi.useRealTimers());

test("an unobserved row page is collected after thirty seconds", async () => {
  vi.useFakeTimers();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const key = ["viewer", "tab", 7, "file", 0, "rows", "unified", "compact", 0, 80] as const;
  await client.fetchQuery({
    queryKey: key,
    queryFn: async () => ({ total: 0, layout: "unified", rows: [] }),
    gcTime: 30_000,
  });

  expect(client.getQueryData(key)).toBeDefined();
  await vi.advanceTimersByTimeAsync(30_001);
  expect(client.getQueryData(key)).toBeUndefined();
});
