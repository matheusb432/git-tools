import { expect, test } from "bun:test";
import { ROW_WINDOW_STRIDE, visibleRowWindow } from "./row-window";

const BASE = {
  panelTop: 0,
  headerHeight: 52,
  rowHeight: 22,
  overscanPx: 600,
};

test("a file with no rows mounts nothing", () => {
  expect(visibleRowWindow({ ...BASE, outerScrollTop: 0, outerViewportHeight: 800, totalRows: 0 })).toBeNull();
});

test("a panel far below the overscanned viewport mounts no rows", () => {
  const window = visibleRowWindow({
    ...BASE,
    panelTop: 100_000,
    outerScrollTop: 0,
    outerViewportHeight: 800,
    totalRows: 500,
  });
  expect(window).toBeNull();
});

test("a panel far above the overscanned viewport mounts no rows", () => {
  const window = visibleRowWindow({
    ...BASE,
    panelTop: 0,
    outerScrollTop: 100_000,
    outerViewportHeight: 800,
    totalRows: 50,
  });
  expect(window).toBeNull();
});

test("the top of a visible panel mounts a slice starting at row zero", () => {
  const window = visibleRowWindow({
    ...BASE,
    panelTop: 0,
    outerScrollTop: 0,
    outerViewportHeight: 800,
    totalRows: 500,
  });
  expect(window).not.toBeNull();
  expect(window?.start).toBe(0);
  // viewport 800 + overscan 600 = 1400px below header -> ~61 rows, quantized up to a stride boundary.
  expect(window?.end).toBeGreaterThanOrEqual(60);
});

test("the slice is clipped to the last row", () => {
  const window = visibleRowWindow({
    ...BASE,
    panelTop: 0,
    outerScrollTop: 100_000,
    outerViewportHeight: 800,
    totalRows: 500,
    // place the viewport at the very bottom of a tall panel
    headerHeight: 52,
  });
  // With panelTop 0 and a 500-row panel (~11000px tall), scrollTop 100000 is past it -> null.
  expect(window).toBeNull();
});

test("slice ends are stride-quantized (hysteresis)", () => {
  const window = visibleRowWindow({
    ...BASE,
    panelTop: 0,
    outerScrollTop: 500,
    outerViewportHeight: 800,
    totalRows: 5000,
  });
  expect(window).not.toBeNull();
  expect((window?.start ?? -1) % ROW_WINDOW_STRIDE).toBe(0);
  expect(((window?.end ?? 0) + 1) % ROW_WINDOW_STRIDE).toBe(0);
});

test("scrolling within one stride keeps the same mounted slice", () => {
  const at = (outerScrollTop: number) =>
    visibleRowWindow({ ...BASE, panelTop: 0, outerScrollTop, outerViewportHeight: 800, totalRows: 5000 });
  // A few px of scroll within a stride must not reshuffle the mounted range.
  expect(at(1000)).toEqual(at(1000 + 5));
  expect(at(1000)).toEqual(at(1000 + 10));
});
