import { expect, test } from "bun:test";
import {
  ESTIMATED_ROW_HEIGHT_PX,
  FILE_PANEL_CHROME_PADDING_PX,
  FILE_PANEL_HEADER_HEIGHT_PX,
  estimatePanelHeight,
  estimateRowCount,
} from "./panel-size";

test("unified row estimate counts every changed line plus hunk overhead", () => {
  expect(estimateRowCount({ added: 10, removed: 10, layout: "unified" })).toBe(28); // ceil(20 * 1.4)
});

test("split row estimate pairs changes at the midpoint before overhead", () => {
  expect(estimateRowCount({ added: 10, removed: 10, layout: "split" })).toBe(14); // ceil(ceil(20/2) * 1.4)
});

test("a file with no changes estimates zero rows", () => {
  expect(estimateRowCount({ added: 0, removed: 0, layout: "unified" })).toBe(0);
  expect(estimateRowCount({ added: 0, removed: 0, layout: "split" })).toBe(0);
});

test("row estimate is monotonic in changed lines", () => {
  const small = estimateRowCount({ added: 5, removed: 5, layout: "unified" });
  const large = estimateRowCount({ added: 50, removed: 50, layout: "unified" });
  expect(large).toBeGreaterThan(small);
});

test("negative or fractional metadata is normalized, not trusted", () => {
  expect(estimateRowCount({ added: -5, removed: 3.9, layout: "unified" })).toBe(
    estimateRowCount({ added: 0, removed: 3, layout: "unified" }),
  );
});

test("collapsed panel height ignores changed lines and reserves only chrome", () => {
  const chrome = FILE_PANEL_HEADER_HEIGHT_PX + FILE_PANEL_CHROME_PADDING_PX;
  expect(estimatePanelHeight({ added: 999, removed: 999, layout: "unified", collapsed: true })).toBe(chrome);
});

test("expanded panel height is chrome plus estimated rows", () => {
  const rows = estimateRowCount({ added: 10, removed: 10, layout: "unified" });
  const expected = FILE_PANEL_HEADER_HEIGHT_PX + FILE_PANEL_CHROME_PADDING_PX + rows * ESTIMATED_ROW_HEIGHT_PX;
  expect(estimatePanelHeight({ added: 10, removed: 10, layout: "unified", collapsed: false })).toBe(expected);
});
