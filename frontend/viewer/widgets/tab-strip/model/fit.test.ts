import { expect, test } from "bun:test";
import type { Recipe, TabMeta } from "@/shared/api";
import type { ArtifactTab, ViewerTab } from "@/entities/diff-tab";
import { nativeOpeningTab, nativeReadyTab } from "@/entities/diff-tab";
import { splitTabs, viewerTabKey } from "./fit";

const recipe: Recipe = {
  source: { kind: "LocalRepo", value: "/repo" },
  op: { op: "diff", target: { target: "unpushed" } },
};
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

const artifact = (n: number): ArtifactTab => ({
  kind: "artifact",
  url: `diff://repo/${n}`,
  label: `t${n}`,
  committedAt: "2026-01-01T00:00:00Z",
});

const native = (n: number) =>
  nativeReadyTab(nativeOpeningTab(`local-${n}`, recipe, `batch-${n}`), {
    tab_id: 100 + n,
    meta: { ...meta, tab_id: 100 + n, batch_id: `batch-${n}` },
  });

const tabs = (n: number): ViewerTab[] => Array.from({ length: n }, (_, i) => artifact(i));

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
  expect(r.visible.map((t) => t.label)).toEqual(["t0", "t1", "t2", "t3"]);
});
test("active in overflow range is pulled into the last visible slot", () => {
  const r = splitTabs(tabs(9), 7, 900, 160, 200);
  expect(r.visible.map((t) => t.label)).toEqual(["t0", "t1", "t2", "t7"]);
  expect(r.overflow.map((t) => t.label)).toContain("t3");
  expect(r.overflow.map((t) => t.label)).not.toContain("t7");
  expect(r.visible.length).toBe(4);
});
test("at least one visible even in a tiny container", () => {
  const r = splitTabs(tabs(5), 4, 100, 160, 200);
  expect(r.visible.length).toBe(1);
  expect(r.visible[0]!.label).toBe("t4");
});

test("viewerTabKey distinguishes artifact and native tabs", () => {
  expect(viewerTabKey(artifact(1))).toBe("artifact:diff://repo/1");
  expect(viewerTabKey(native(2))).toBe("native:local-2");
});

test("active native tab pulled into visible keeps its stable native key", () => {
  const mixedTabs: ViewerTab[] = [artifact(0), artifact(1), artifact(2), artifact(3), native(9)];
  const r = splitTabs(mixedTabs, 4, 900, 160, 200);
  expect(r.visible.map(viewerTabKey)).toEqual([
    "artifact:diff://repo/0",
    "artifact:diff://repo/1",
    "artifact:diff://repo/2",
    "native:local-9",
  ]);
  expect(r.overflow.map(viewerTabKey)).toContain("artifact:diff://repo/3");
});
