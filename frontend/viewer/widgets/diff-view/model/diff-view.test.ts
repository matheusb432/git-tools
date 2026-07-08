import { expect, test } from "bun:test";
import { emptyRowPageCache, putRows, rowPageKey } from "@/entities/diff-tab";
import type { UnifiedRow } from "@/shared/api";
import {
  activePaneKey,
  activePanelFileIndexes,
  allFilePanelsCollapsed,
  clearCommitFocus,
  copyPageAccumulatorFromCache,
  copyUnifiedRows,
  createDiffViewState,
  expandFilePanel,
  filePanelIsExpanded,
  focusCommit,
  fullFromSetting,
  layoutFromSetting,
  missingCopyPageRequestsForAccumulator,
  missingCopyPageRequests,
  nextFileIndex,
  orderedCopyPages,
  orderedCopyPagesFromAccumulator,
  putCopyPageInAccumulator,
  settingFromFull,
  settingFromLayout,
  toggleAllFilePanels,
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

test("active panel rendering is limited to the selected visible file", () => {
  expect(activePanelFileIndexes({ visibleFileIndexes: [0, 1, 2], activeFileIdx: 1 })).toEqual([1]);
  expect(activePanelFileIndexes({ visibleFileIndexes: [0, 1, 2], activeFileIdx: null })).toEqual([0]);
  expect(activePanelFileIndexes({ visibleFileIndexes: [], activeFileIdx: null })).toEqual([]);
});

test("fold-all collapses visible file panels and selected file expansion is explicit", () => {
  const folded = toggleAllFilePanels({ collapsedFileIdxs: new Set<number>() }, [0, 1, 2]);

  expect(allFilePanelsCollapsed(folded, [0, 1, 2])).toBe(true);
  expect(filePanelIsExpanded(folded, 1)).toBe(false);

  const selectedExpanded = expandFilePanel(folded, 1);
  expect(filePanelIsExpanded(selectedExpanded, 1)).toBe(true);
  expect(filePanelIsExpanded(selectedExpanded, 2)).toBe(false);
});

test("fold-all toggles back to expanded when every visible file is collapsed", () => {
  const folded = { collapsedFileIdxs: new Set([0, 1]) };
  const expanded = toggleAllFilePanels(folded, [0, 1]);

  expect(allFilePanelsCollapsed(expanded, [0, 1])).toBe(false);
  expect(filePanelIsExpanded(expanded, 0)).toBe(true);
  expect(filePanelIsExpanded(expanded, 1)).toBe(true);
});

test("copy page accumulator can fill all selected-file gaps without mutating row cache", () => {
  const cache = putRows(
    emptyRowPageCache(),
    rowPageKey({ tabId: 7, fileIdx: 0, layout: "unified", full: false, pageStart: 0, pageSize: 80 }),
    { total: 240, layout: "unified", rows: [] },
  );
  const accumulator = copyPageAccumulatorFromCache({
    rowPageCache: cache,
    tabId: 7,
    fileIdx: 0,
    layout: "unified",
    full: false,
  });
  const filled = putCopyPageInAccumulator(
    putCopyPageInAccumulator(accumulator, 80, { total: 240, layout: "unified", rows: [] }),
    160,
    { total: 240, layout: "unified", rows: [] },
  );

  expect(cache.size).toBe(1);
  expect(missingCopyPageRequestsForAccumulator({ pages: filled, pageSize: 80, total: 240 })).toEqual([]);
  expect(orderedCopyPagesFromAccumulator(filled)).toHaveLength(3);
});
