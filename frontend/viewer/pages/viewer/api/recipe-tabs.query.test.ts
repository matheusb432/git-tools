import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import type { ApiTransport } from "@/shared/api";
import { renderWithData, tabMeta } from "@/shared/test";
import RecipeTabsMutationHarness from "./RecipeTabsMutationHarness.svelte";
import { tabMetaKey } from "./recipe-tabs.svelte";

const mountedTest = process.env.VITEST === "true" ? test : test.skip;

type TransportOperation = {
  readonly operation: string;
  readonly input: Record<string, unknown>;
};

mountedTest("open seeds the exact tab metadata cache before any metadata observer is mounted", async () => {
  const operations: TransportOperation[] = [];
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "open_recipe") return { tab_id: 7, meta: tabMeta };
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(RecipeTabsMutationHarness, {}, transport);

  await user.click(screen.getByRole("button", { name: "Open recipe" }));

  await waitFor(() => expect(screen.getByTestId("open-status")).toHaveTextContent("success"));
  expect(queryClient.getQueryData(tabMetaKey(7))).toEqual(tabMeta);
  expect(operations).toEqual([
    {
      operation: "open_recipe",
      input: { recipe: expect.any(Object), batchId: "batch-1" },
    },
  ]);
  expect(
    queryClient
      .getMutationCache()
      .getAll()
      .map((mutation) => mutation.options.mutationKey),
  ).toContainEqual(["viewer", "tab", "open"]);
});

mountedTest("failed refresh preserves cached metadata and records a tab-prefixed mutation", async () => {
  const operations: TransportOperation[] = [];
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "refresh_tab") throw new Error("refresh failed");
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(RecipeTabsMutationHarness, {}, transport);
  queryClient.setQueryData(tabMetaKey(7), tabMeta);

  await user.click(screen.getByRole("button", { name: "Refresh tab" }));

  await waitFor(() => expect(screen.getByTestId("refresh-status")).toHaveTextContent("error"));
  expect(queryClient.getQueryData(tabMetaKey(7))).toEqual(tabMeta);
  expect(operations).toEqual([{ operation: "refresh_tab", input: { tabId: 7 } }]);
  expect(
    queryClient
      .getMutationCache()
      .getAll()
      .map((mutation) => mutation.options.mutationKey),
  ).toContainEqual(["viewer", "tab", 7, "refresh"]);
});
