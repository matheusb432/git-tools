import { expect, test } from "bun:test";
import { closeTabState, type TabState } from "../tab-state";

const state: TabState = {
  tabs: [
    { url: "diff://repo/a", label: "a" },
    { url: "diff://repo/b", label: "b" },
    { url: "diff://repo/c", label: "c" },
  ],
  active: 1,
  showHistory: false,
};

test("closing the active tab selects the next tab at the same index", () => {
  expect(closeTabState(state, 1)).toEqual({
    tabs: [
      { url: "diff://repo/a", label: "a" },
      { url: "diff://repo/c", label: "c" },
    ],
    active: 1,
    showHistory: false,
  });
});

test("closing a tab before the active tab preserves the active tab identity", () => {
  expect(closeTabState(state, 0)).toEqual({
    tabs: [
      { url: "diff://repo/b", label: "b" },
      { url: "diff://repo/c", label: "c" },
    ],
    active: 0,
    showHistory: false,
  });
});

test("closing the last open tab leaves a stable empty state", () => {
  expect(
    closeTabState(
      {
        tabs: [{ url: "diff://repo/a", label: "a" }],
        active: 0,
        showHistory: false,
      },
      0
    )
  ).toEqual({ tabs: [], active: 0, showHistory: false });
});
