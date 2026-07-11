import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import type { ApiTransport, RowsPage } from "@/shared/api";
import { renderWithData, recipe, setVirtualListGeometry, twoFileTabMeta } from "@/shared/test";
import { TEST_IDS } from "@/shared/testids";
import { createDiffReviewStore } from "../../model/diff-review";
import DiffView from "./DiffView.svelte";

const mountedTest = process.env.VITEST === "true" ? test : test.skip;

type RowRequest = {
  readonly fileIdx: number;
  readonly layout: "unified" | "split";
  readonly density: "compact" | "full";
};

function rowPage(request: RowRequest): RowsPage {
  const text = `file-${request.fileIdx} ${request.layout} ${request.density}`;
  if (request.layout === "split") {
    return { total: 1, layout: "split", rows: [{ kind: "hunk", text }] };
  }
  return {
    total: 1,
    layout: "unified",
    rows: [{ kind: "hunk", old_no: null, new_no: null, text, owner: null, long_len: null }],
  };
}

function rowRequest(input: Record<string, unknown>): RowRequest {
  const { fileIdx, layout } = input;
  if (typeof fileIdx !== "number") throw new Error("file_rows requires a numeric fileIdx");
  if (layout !== "unified" && layout !== "split") throw new Error("file_rows requires a supported layout");
  return { fileIdx, layout, density: input.full === true ? "full" : "compact" };
}

mountedTest("two small files settle across persisted pane transitions without scrolling", async () => {
  const rowRequests: RowRequest[] = [];
  const transport: ApiTransport = {
    invoke: async (operation, input = {}) => {
      if (operation === "get_setting") return input.key === "diff.layout" ? "split" : "full";
      if (operation === "set_setting") return null;
      if (operation === "file_rows") {
        const request = rowRequest(input);
        rowRequests.push(request);
        return rowPage(request);
      }
      throw new Error(`Unexpected operation: ${operation}`);
    },
    listen: async () => () => undefined,
  };
  const user = userEvent.setup();
  const tab = {
    localId: "tab-7",
    recipe,
    batchId: "batch-1",
    tabId: 7,
    isNew: false,
    failure: null,
    live: null,
    meta: twoFileTabMeta,
    isRefreshing: false,
  };

  renderWithData(DiffView, { tab, reviewStore: createDiffReviewStore(), onRefresh: async () => undefined }, transport);
  const fileList = await screen.findByTestId(TEST_IDS.diffView.fileList);
  setVirtualListGeometry(fileList);

  expect(await screen.findByRole("button", { name: "Collapse src/file-0.ts" })).toBeVisible();
  expect(await screen.findByRole("button", { name: "Collapse src/file-1.ts" })).toBeVisible();
  expect(await screen.findByText("file-0 split full")).toBeVisible();
  expect(await screen.findByText("file-1 split full")).toBeVisible();
  expect(screen.queryAllByTestId(TEST_IDS.diffView.rowSkeleton)).toHaveLength(0);

  await user.click(screen.getByRole("radio", { name: "Unified layout" }));

  expect(await screen.findByText("file-0 unified full")).toBeVisible();
  expect(await screen.findByText("file-1 unified full")).toBeVisible();
  expect(screen.queryByText("file-0 split full")).not.toBeInTheDocument();
  expect(screen.queryByText("file-1 split full")).not.toBeInTheDocument();
  expect(screen.queryAllByTestId(TEST_IDS.diffView.rowSkeleton)).toHaveLength(0);

  await user.click(screen.getByRole("radio", { name: "Compact diff rows" }));

  expect(await screen.findByText("file-0 unified compact")).toBeVisible();
  expect(await screen.findByText("file-1 unified compact")).toBeVisible();
  expect(screen.queryByText("file-0 unified full")).not.toBeInTheDocument();
  expect(screen.queryByText("file-1 unified full")).not.toBeInTheDocument();
  expect(screen.queryAllByTestId(TEST_IDS.diffView.rowSkeleton)).toHaveLength(0);
  await waitFor(() => {
    expect(rowRequests).toEqual(
      expect.arrayContaining([
        { fileIdx: 0, layout: "split", density: "full" },
        { fileIdx: 1, layout: "split", density: "full" },
        { fileIdx: 0, layout: "unified", density: "compact" },
        { fileIdx: 1, layout: "unified", density: "compact" },
      ]),
    );
  });
});
