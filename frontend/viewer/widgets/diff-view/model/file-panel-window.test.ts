import { expect, test } from "bun:test";
import {
  FILE_PANEL_MIN_WINDOW,
  FILE_PANEL_WINDOW_STEP,
  filePanelDomKey,
  filePanelWindowIndexes,
} from "./file-panel-window";

test("file panel constants keep the windowing target explicit", () => {
  expect(FILE_PANEL_MIN_WINDOW).toBe(20);
  expect(FILE_PANEL_WINDOW_STEP).toBe(5);
});

test("file panel keys are stable by file index and path", () => {
  expect(filePanelDomKey({ fileIdx: 3, path: "src/main.rs" })).toBe("3:src/main.rs");
});

test("window renders at least the minimum number of panels", () => {
  const indexes = filePanelWindowIndexes({ startIndex: 0, endIndex: 2, count: 100 });
  expect(indexes.length).toBe(FILE_PANEL_MIN_WINDOW);
  expect(indexes[0]).toBe(0);
});

test("window never exceeds the available file count", () => {
  const indexes = filePanelWindowIndexes({ startIndex: 0, endIndex: 2, count: 6 });
  expect(indexes).toEqual([0, 1, 2, 3, 4, 5]);
});

test("window start only re-anchors on step boundaries (hysteresis)", () => {
  // Scrolling within a single 5-file stride keeps the same anchor.
  const withinStride = [1, 2, 3, 4].map(
    (startIndex) => filePanelWindowIndexes({ startIndex, endIndex: startIndex + 2, count: 100 })[0],
  );
  expect(withinStride).toEqual([0, 0, 0, 0]);

  // Crossing the stride boundary shifts the anchor by exactly one stride.
  expect(filePanelWindowIndexes({ startIndex: 5, endIndex: 7, count: 100 })[0]).toBe(5);
  expect(filePanelWindowIndexes({ startIndex: 10, endIndex: 12, count: 100 })[0]).toBe(10);
});

test("window always covers the visible range", () => {
  const indexes = filePanelWindowIndexes({ startIndex: 50, endIndex: 78, count: 200 });
  expect(indexes[0]).toBeLessThanOrEqual(50);
  expect(indexes[indexes.length - 1]).toBeGreaterThanOrEqual(78);
});

test("window clamps to the last file without dropping below the minimum", () => {
  const indexes = filePanelWindowIndexes({ startIndex: 95, endIndex: 99, count: 100 });
  expect(indexes[indexes.length - 1]).toBe(99);
  expect(indexes.length).toBeGreaterThanOrEqual(FILE_PANEL_MIN_WINDOW);
});

test("empty file list renders no panels", () => {
  expect(filePanelWindowIndexes({ startIndex: 0, endIndex: 0, count: 0 })).toEqual([]);
});
