import { expect, test } from "bun:test";
import type { SplitCell } from "@/shared/api";
import {
  panePageErrorEntries,
  retryablePanePageRequests,
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

test("pane error entries only include the active pane", () => {
  const pageErrors = new Map<string, string>([
    ["7:0:unified:compact:0:80", "unified compact failed"],
    ["7:0:split:compact:0:80", "split compact failed"],
    ["7:0:unified:full:0:80", "unified full failed"],
  ]);

  expect(Array.from(panePageErrorEntries(pageErrors, "7:0:split:compact"))).toEqual([
    ["7:0:split:compact:0:80", "split compact failed"],
  ]);
});

test("retry requests only target active-pane page errors", () => {
  const pageErrors = new Map<string, string>([
    ["7:0:unified:compact:0:80", "first unified page failed"],
    ["7:0:unified:compact:80:80", "second unified page failed"],
    ["7:0:split:compact:0:80", "split page failed"],
  ]);

  expect(retryablePanePageRequests(pageErrors, "7:0:unified:compact")).toEqual([
    { pageStart: 0, pageSize: 80 },
    { pageStart: 80, pageSize: 80 },
  ]);
});
