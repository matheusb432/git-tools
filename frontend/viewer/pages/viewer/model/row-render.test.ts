import { expect, test } from "bun:test";
import type { SplitCell } from "@/shared/api";
import {
  expandedLongRowKeysForFile,
  expandedLongRowsForPane,
  scopedLongRowKey,
  siblingLayout,
  splitLongRowKey,
  splitMarker,
  unifiedLongRowKey,
} from "./row-render";

test("missing split sides render an empty marker gutter", () => {
  expect(splitMarker("old", null)).toBe("");
  expect(splitMarker("new", null)).toBe("");

  const cell: SplitCell = { no: 1, text: "line", owner: null, spans: [], long_len: null };
  expect(splitMarker("old", cell)).toBe("-");
  expect(splitMarker("new", cell)).toBe("+");
});

test("sibling layout warms the opposite pane", () => {
  expect(siblingLayout("unified")).toBe("split");
  expect(siblingLayout("split")).toBe("unified");
});

test("long-row expansion keys stay unique across absolute row indexes", () => {
  expect(unifiedLongRowKey(0)).not.toBe(unifiedLongRowKey(1));
  expect(splitLongRowKey(12, "old")).not.toBe(splitLongRowKey(12, "new"));
  expect(splitLongRowKey(12, "old")).not.toBe(splitLongRowKey(13, "old"));
});

test("stored long-row expansion keys stay isolated by file pane", () => {
  const firstPane = "7:1:unified:compact";
  const secondPane = "7:2:unified:compact";
  const expanded = new Set([scopedLongRowKey(firstPane, "u:4"), scopedLongRowKey(secondPane, "u:4")]);

  const firstFileKeys = expandedLongRowKeysForFile(expanded, 7, 1);
  expect(firstFileKeys).toEqual(new Set([scopedLongRowKey(firstPane, "u:4")]));
  expect(expandedLongRowsForPane(firstFileKeys, firstPane)).toEqual(new Set(["u:4"]));
  expect(expandedLongRowsForPane(firstFileKeys, secondPane)).toEqual(new Set());
});
