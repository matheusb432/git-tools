import { expect, test } from "bun:test";
import { emptyRowPageCache, putRows, rowPageKey } from "@/entities/diff-tab";
import type { UnifiedRow } from "@/shared/api";
import {
  activePaneKey,
  clearCommitFocus,
  copyUnifiedRows,
  createDiffViewState,
  focusCommit,
  fullFromSetting,
  layoutFromSetting,
  missingCopyPageRequests,
  nextFileIndex,
  orderedCopyPages,
  settingFromFull,
  settingFromLayout,
  toggleLayout,
} from "./diff-view";

test("default view state starts in unified compact mode", () => {
  expect(createDiffViewState().layout).toBe("unified");
  expect(createDiffViewState().full).toBe(false);
});

test("toggleLayout switches between unified and split", () => {
  expect(toggleLayout(createDiffViewState()).layout).toBe("split");
});

test("focused commit filters files and can be cleared", () => {
  const state = focusCommit(createDiffViewState(), "abc1234");
  expect(state.focusedCommits.has("abc1234")).toBe(true);
  expect(clearCommitFocus(state).focusedCommits.size).toBe(0);
});

test("copy text includes visible row markers", () => {
  const rows: UnifiedRow[] = [{ kind: "add", old_no: null, new_no: 1, text: "new", owner: null, long_len: null }];
  expect(copyUnifiedRows(rows)).toBe("+new");
});

test("active pane key changes with layout", () => {
  expect(activePaneKey({ tabId: 7, fileIdx: 0, layout: "unified", full: false })).toBe("7:0:unified:compact");
  expect(activePaneKey({ tabId: 7, fileIdx: 0, layout: "split", full: false })).toBe("7:0:split:compact");
});

test("normalizes stored layout setting", () => {
  expect(layoutFromSetting("split")).toBe("split");
  expect(layoutFromSetting("bogus")).toBe("unified");
  expect(layoutFromSetting(null)).toBe("unified");
});

test("normalizes stored full setting", () => {
  expect(fullFromSetting("full")).toBe(true);
  expect(fullFromSetting("compact")).toBe(false);
  expect(fullFromSetting("bogus")).toBe(false);
  expect(fullFromSetting(null)).toBe(false);
});

test("serializes layout and full settings", () => {
  expect(settingFromLayout("unified")).toBe("unified");
  expect(settingFromLayout("split")).toBe("split");
  expect(settingFromFull(false)).toBe("compact");
  expect(settingFromFull(true)).toBe("full");
});

test("arrow navigation clamps to available files", () => {
  expect(nextFileIndex({ current: 0, direction: "previous", total: 3 })).toBe(0);
  expect(nextFileIndex({ current: 0, direction: "next", total: 3 })).toBe(1);
  expect(nextFileIndex({ current: 2, direction: "next", total: 3 })).toBe(2);
});

test("ordered copy pages sort by page start and ignore other panes", () => {
  const cache = putRows(
    putRows(
      putRows(
        emptyRowPageCache(),
        rowPageKey({ tabId: 7, fileIdx: 0, layout: "unified", full: false, pageStart: 80, pageSize: 80 }),
        { total: 120, layout: "unified", rows: [] },
      ),
      rowPageKey({ tabId: 7, fileIdx: 0, layout: "unified", full: false, pageStart: 0, pageSize: 80 }),
      { total: 120, layout: "unified", rows: [] },
    ),
    rowPageKey({ tabId: 7, fileIdx: 1, layout: "unified", full: false, pageStart: 0, pageSize: 80 }),
    { total: 40, layout: "unified", rows: [] },
  );

  expect(
    orderedCopyPages({
      rowPageCache: cache,
      tabId: 7,
      fileIdx: 0,
      layout: "unified",
      full: false,
    }).map((page) => page.pageStart),
  ).toEqual([0, 80]);
});

test("missing copy page requests include only gaps for the selected file", () => {
  const cache = putRows(
    putRows(
      emptyRowPageCache(),
      rowPageKey({ tabId: 7, fileIdx: 0, layout: "split", full: true, pageStart: 0, pageSize: 80 }),
      { total: 200, layout: "split", rows: [] },
    ),
    rowPageKey({ tabId: 7, fileIdx: 0, layout: "split", full: true, pageStart: 160, pageSize: 80 }),
    { total: 200, layout: "split", rows: [] },
  );

  expect(
    missingCopyPageRequests({
      rowPageCache: cache,
      tabId: 7,
      fileIdx: 0,
      layout: "split",
      full: true,
      pageSize: 80,
      total: 200,
    }),
  ).toEqual([{ pageStart: 80, pageSize: 80 }]);
});
