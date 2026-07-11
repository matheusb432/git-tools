import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import type { ApiTransport } from "@/shared/api";
import { renderWithData } from "@/shared/test";
import SettingsQueryHarness from "./SettingsQueryHarness.svelte";
import { type ViewerSettings, viewerSettingsKey } from "./settings.svelte";

const mountedTest = process.env.VITEST === "true" ? test : test.skip;

type TransportOperation = {
  readonly operation: string;
  readonly input: Record<string, unknown>;
};

mountedTest("settings query decodes defaults into the shared resource cache", async () => {
  const operations: TransportOperation[] = [];
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "get_setting") return null;
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };

  const { queryClient } = renderWithData(SettingsQueryHarness, {}, transport);

  await screen.findByText("unified:compact");
  await waitFor(() => expect(screen.getByTestId("settings-status")).toHaveTextContent("success"));
  expect(queryClient.getQueryData<ViewerSettings>(viewerSettingsKey)).toEqual({
    layout: "unified",
    density: "compact",
  });
  expect(
    queryClient.getQueryCache().find({ queryKey: viewerSettingsKey, exact: true })?.observers[0]?.options.staleTime,
  ).toBe(Infinity);
  expect(operations).toEqual([
    { operation: "get_setting", input: { key: "diff.layout" } },
    { operation: "get_setting", input: { key: "diff.full" } },
  ]);
});

mountedTest("an errored refetch shows defaults while retaining prior settings in the cache", async () => {
  const operations: TransportOperation[] = [];
  let readState: "success" | "error" = "success";
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation !== "get_setting") throw new Error(`Unexpected operation: ${operation}`);
      if (readState === "error") throw new Error("settings unavailable");
      return input.key === "diff.layout" ? "split" : "full";
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(SettingsQueryHarness, {}, transport);
  await screen.findByText("split:full");

  readState = "error";
  await user.click(screen.getByRole("button", { name: "Refetch settings" }));

  await waitFor(() => expect(screen.getByTestId("settings-status")).toHaveTextContent("error"));
  expect(queryClient.getQueryData<ViewerSettings>(viewerSettingsKey)).toEqual({
    layout: "split",
    density: "full",
  });
  expect(screen.getByTestId("settings-value")).toHaveTextContent("unified:compact");
  expect(operations).toEqual([
    { operation: "get_setting", input: { key: "diff.layout" } },
    { operation: "get_setting", input: { key: "diff.full" } },
    { operation: "get_setting", input: { key: "diff.layout" } },
    { operation: "get_setting", input: { key: "diff.full" } },
  ]);
});

mountedTest("setting mutation updates the cache optimistically and restores its snapshot on failure", async () => {
  const operations: TransportOperation[] = [];
  let rejectWrite: ((reason: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      operations.push({ operation, input });
      if (operation === "get_setting") return null;
      if (operation === "set_setting") {
        return new Promise<never>((_resolve, reject) => {
          rejectWrite = reject;
        });
      }
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(SettingsQueryHarness, {}, transport);
  await screen.findByText("unified:compact");

  await user.click(screen.getByRole("button", { name: "Use split layout" }));

  await waitFor(() => {
    expect(queryClient.getQueryData<ViewerSettings>(viewerSettingsKey)).toEqual({
      layout: "split",
      density: "compact",
    });
  });
  expect(operations.at(-1)).toEqual({
    operation: "set_setting",
    input: { key: "diff.layout", value: "split" },
  });
  expect(
    queryClient
      .getMutationCache()
      .getAll()
      .map((mutation) => mutation.options.scope),
  ).toEqual([{ id: "viewer-settings:layout" }]);

  rejectWrite?.(new Error("disk full"));

  await waitFor(() => {
    expect(queryClient.getQueryData<ViewerSettings>(viewerSettingsKey)).toEqual({
      layout: "unified",
      density: "compact",
    });
  });
  expect(screen.getByTestId("settings-mutation-status")).toHaveTextContent("error");
});

mountedTest("queued setting writes reconcile each field to persisted outcomes", async () => {
  let persistedDensity: ViewerSettings["density"] = "compact";
  let rejectLayout: ((reason: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      if (operation === "get_setting") return input.key === "diff.layout" ? "unified" : persistedDensity;
      if (operation !== "set_setting") throw new Error(`Unexpected operation: ${operation}`);
      if (input.key === "diff.layout") {
        return new Promise<never>((_resolve, reject) => {
          rejectLayout = reject;
        });
      }
      persistedDensity = input.value === "full" ? "full" : "compact";
      return null;
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(SettingsQueryHarness, {}, transport);
  await screen.findByText("unified:compact");

  await user.click(screen.getByRole("button", { name: "Use split layout" }));
  await user.click(screen.getByRole("button", { name: "Use full density" }));
  rejectLayout?.(new Error("layout persistence failed"));

  await waitFor(() => {
    expect(queryClient.getQueryData<ViewerSettings>(viewerSettingsKey)).toEqual({
      layout: "unified",
      density: persistedDensity,
    });
  });
  expect(persistedDensity).toBe("full");
});

mountedTest("a permanently pending layout write does not block density persistence", async () => {
  const writes: Record<string, unknown>[] = [];
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      if (operation === "get_setting") return null;
      if (operation !== "set_setting") throw new Error(`Unexpected operation: ${operation}`);
      writes.push(input);
      if (input.key === "diff.layout") return new Promise<never>(() => undefined);
      return null;
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(SettingsQueryHarness, {}, transport);
  await screen.findByText("unified:compact");

  await user.click(screen.getByRole("button", { name: "Use split layout" }));
  await user.click(screen.getByRole("button", { name: "Use full density" }));

  await waitFor(() =>
    expect(writes).toContainEqual({
      key: "diff.full",
      value: "full",
    }),
  );
  expect(queryClient.getQueryData<ViewerSettings>(viewerSettingsKey)).toEqual({
    layout: "split",
    density: "full",
  });
  expect(
    queryClient
      .getMutationCache()
      .getAll()
      .map((mutation) => mutation.options.scope),
  ).toEqual([{ id: "viewer-settings:layout" }, { id: "viewer-settings:density" }]);
});

mountedTest("queued writes to the same setting preserve the latest successful value", async () => {
  let writeCount = 0;
  let rejectFirst: ((reason: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async (operation) => {
      if (operation === "get_setting") return null;
      if (operation !== "set_setting") throw new Error(`Unexpected operation: ${operation}`);
      writeCount += 1;
      if (writeCount === 1) {
        return new Promise<never>((_resolve, reject) => {
          rejectFirst = reject;
        });
      }
      return null;
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const { queryClient } = renderWithData(SettingsQueryHarness, {}, transport);
  await screen.findByText("unified:compact");

  await user.click(screen.getByRole("button", { name: "Use split layout" }));
  await user.click(screen.getByRole("button", { name: "Use split layout" }));
  rejectFirst?.(new Error("first write failed"));

  await waitFor(() => expect(writeCount).toBe(2));
  await waitFor(() => {
    expect(queryClient.getQueryData<ViewerSettings>(viewerSettingsKey)).toEqual({
      layout: "split",
      density: "compact",
    });
  });
});
