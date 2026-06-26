import { expect, test } from "bun:test";
import { closeAllTabs, closeOthers, closeTabState, type TabState } from "./tab";

const tab = (n: string) => ({ url: `diff://r/${n}`, label: n });
const state = (active: number): TabState => ({ tabs: [tab("a"), tab("b"), tab("c")], active, showHistory: false });

test("closeTabState before active shifts active left", () => {
  const r = closeTabState(state(2), 0);
  expect(r.tabs.map((t) => t.label)).toEqual(["b", "c"]);
  expect(r.active).toBe(1);
});
test("closeTabState of active clamps active", () => {
  const r = closeTabState(state(2), 2);
  expect(r.tabs.map((t) => t.label)).toEqual(["a", "b"]);
  expect(r.active).toBe(1);
});
test("closeTabState out of range is a no-op", () => {
  expect(closeTabState(state(0), 9)).toEqual(state(0));
});
test("closeAllTabs empties and resets", () => {
  const r = closeAllTabs(state(2));
  expect(r.tabs).toEqual([]);
  expect(r.active).toBe(0);
});
test("closeOthers keeps only the kept tab, active 0", () => {
  const r = closeOthers(state(0), 2);
  expect(r.tabs.map((t) => t.label)).toEqual(["c"]);
  expect(r.active).toBe(0);
});
test("closeOthers out of range is a no-op", () => {
  expect(closeOthers(state(0), 9)).toEqual(state(0));
});
test("closeTabState after active keeps active in place", () => {
  const r = closeTabState(state(0), 2);
  expect(r.tabs.map((t) => t.label)).toEqual(["a", "b"]);
  expect(r.active).toBe(0);
});
