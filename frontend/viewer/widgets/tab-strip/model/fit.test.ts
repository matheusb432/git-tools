import { expect, test } from "bun:test";
import type { ArtifactTab, ViewerTab } from "@/entities/diff-tab";
import { splitTabs, viewerTabKey } from "./fit";

const artifact = (n: number): ArtifactTab => ({
  kind: "artifact",
  url: `diff://repo/${n}`,
  label: `t${n}`,
  committedAt: "2026-01-01T00:00:00Z",
});

const tabs = (n: number): ViewerTab[] => Array.from({ length: n }, (_, i) => artifact(i));

function artifactLabel(tab: ViewerTab): string {
  return tab.label;
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
  expect(r.visible.map(artifactLabel)).toEqual(["t0", "t1", "t2", "t3"]);
});
test("active in overflow range is pulled into the last visible slot", () => {
  const r = splitTabs(tabs(9), 7, 900, 160, 200);
  expect(r.visible.map(artifactLabel)).toEqual(["t0", "t1", "t2", "t7"]);
  expect(r.overflow.map(artifactLabel)).toContain("t3");
  expect(r.overflow.map(artifactLabel)).not.toContain("t7");
  expect(r.visible.length).toBe(4);
});
test("at least one visible even in a tiny container", () => {
  const r = splitTabs(tabs(5), 4, 100, 160, 200);
  expect(r.visible.length).toBe(1);
  expect(artifactLabel(r.visible[0]!)).toBe("t4");
});

test("viewerTabKey keys artifact tabs by url", () => {
  expect(viewerTabKey(artifact(1))).toBe("artifact:diff://repo/1");
});

test("active tab pulled into visible keeps its stable key", () => {
  const r = splitTabs(tabs(5), 4, 900, 160, 200);
  expect(r.visible.map(viewerTabKey)).toEqual([
    "artifact:diff://repo/0",
    "artifact:diff://repo/1",
    "artifact:diff://repo/2",
    "artifact:diff://repo/4",
  ]);
  expect(r.overflow.map(viewerTabKey)).toContain("artifact:diff://repo/3");
});
