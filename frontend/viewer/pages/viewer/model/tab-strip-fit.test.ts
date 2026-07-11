import { expect, test } from "bun:test";
import type { TabMeta } from "@/shared/api";
import { splitTabs, viewerTabKey } from "./tab-strip-fit";

const meta: TabMeta = {
  tab_id: 7,
  batch_id: "batch-1",
  title: "Native diff",
  repo_name: "repo",
  repo_root: "/repo",
  branch: "topic",
  upstream: "origin/main",
  cmd_lead: "git diff ",
  cmd_range: "origin/main..HEAD",
  cmd_trail: "",
  commits_label: "1 commit",
  foot_cmd: "git diff",
  foot_note: "",
  is_empty: false,
  commits: [],
  files: [],
};

type Tab = { readonly localId: string; readonly meta: TabMeta };

const native = (n: number): Tab => ({
  localId: `local-${n}`,
  meta: { ...meta, title: `t${n}`, tab_id: 100 + n, batch_id: `batch-${n}` },
});

const tabs = (n: number): Tab[] => Array.from({ length: n }, (_, i) => native(i));

function tabTitle(tab: Tab): string {
  return tab.meta.title;
}

test("all fit -> no overflow", () => {
  const r = splitTabs(tabs(3), 0, 1000, 160, 200);
  expect(r.visible.length).toBe(3);
  expect(r.overflow.length).toBe(0);
});
test("overflow split keeps capacity visible", () => {
  // capacity = floor((900-200)/160) = 4
  const r = splitTabs(tabs(9), 0, 900, 160, 200);
  expect(r.visible.length).toBe(4);
  expect(r.overflow.length).toBe(5);
  expect(r.visible.map(tabTitle)).toEqual(["t0", "t1", "t2", "t3"]);
});
test("active in overflow range is pulled into the last visible slot", () => {
  const r = splitTabs(tabs(9), 7, 900, 160, 200);
  expect(r.visible.map(tabTitle)).toEqual(["t0", "t1", "t2", "t7"]);
  expect(r.overflow.map(tabTitle)).toContain("t3");
  expect(r.overflow.map(tabTitle)).not.toContain("t7");
  expect(r.visible.length).toBe(4);
});
test("at least one visible even in a tiny container", () => {
  const r = splitTabs(tabs(5), 4, 100, 160, 200);
  expect(r.visible.length).toBe(1);
  expect(tabTitle(r.visible[0]!)).toBe("t4");
});

test("viewerTabKey keys a tab by its stable localId", () => {
  expect(viewerTabKey(native(2))).toBe("native:local-2");
});

test("active tab pulled into visible keeps its stable native key", () => {
  const mixedTabs: Tab[] = [native(0), native(1), native(2), native(3), native(9)];
  const r = splitTabs(mixedTabs, 4, 900, 160, 200);
  expect(r.visible.map(viewerTabKey)).toEqual(["native:local-0", "native:local-1", "native:local-2", "native:local-9"]);
  expect(r.overflow.map(viewerTabKey)).toContain("native:local-3");
});
