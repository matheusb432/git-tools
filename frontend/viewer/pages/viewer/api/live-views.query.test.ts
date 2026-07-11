import { screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import type { ApiTransport, LiveView, SourceProbe } from "@/shared/api";
import { renderWithData } from "@/shared/test";
import LiveViewsQueryHarness from "./LiveViewsQueryHarness.svelte";
import SourceProbeQueryHarness from "./SourceProbeQueryHarness.svelte";
import { liveViewsKey, sourceProbeKey } from "./live-views.svelte";

const mountedTest = process.env.VITEST === "true" ? test : test.skip;

type TransportOperation = {
  readonly operation: string;
  readonly input: Record<string, unknown>;
};

mountedTest("a malformed live-view entry rejects the complete cached list", async () => {
  const operations: TransportOperation[] = [];
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "list_live_views") {
        return [
          {
            source_kind: "LocalRepo",
            source_value: "/valid",
            display_name: "valid",
            created_at: "2026-07-10T12:00:00Z",
            last_opened_at: null,
          },
          { source_kind: "LocalRepo", source_value: "/malformed" },
        ];
      }
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };

  const { queryClient } = renderWithData(LiveViewsQueryHarness, {}, transport);

  await screen.findByText("error");
  expect(screen.getByTestId("live-views-error")).toHaveTextContent("Malformed list_live_views response");
  expect(queryClient.getQueryData<readonly LiveView[]>(liveViewsKey)).toBeUndefined();
  expect(operations).toEqual([{ operation: "list_live_views", input: {} }]);
});

mountedTest("a disabled source probe stays idle and performs no transport operation", async () => {
  const operations: TransportOperation[] = [];
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      return { outcome: "ok" };
    },
    listen: async () => () => undefined,
  };

  const { queryClient } = renderWithData(
    SourceProbeQueryHarness,
    { source: { kind: "LocalRepo", value: "/idle" }, enabled: false },
    transport,
  );

  await screen.findByText("pending:idle");
  expect(
    queryClient.getQueryCache().find({
      queryKey: sourceProbeKey({ kind: "LocalRepo", value: "/idle" }),
      exact: true,
    })?.observers[0]?.options.enabled,
  ).toBe(false);
  expect(operations).toEqual([]);
});

mountedTest("an enabled source probe uses its complete identity in transport and cache", async () => {
  const operations: TransportOperation[] = [];
  const source = { kind: "LocalRepo", value: "/repo" };
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      return { outcome: "ok" };
    },
    listen: async () => () => undefined,
  };

  const { queryClient } = renderWithData(SourceProbeQueryHarness, { source, enabled: true }, transport);

  await screen.findByText("ok");
  expect(queryClient.getQueryData<SourceProbe>(sourceProbeKey(source))).toEqual({ outcome: "ok" });
  expect(operations).toEqual([
    {
      operation: "probe_source",
      input: { sourceKind: "LocalRepo", sourceValue: "/repo" },
    },
  ]);
});
