import { expect, test } from "bun:test";
import { splitTabs } from "./fit";

const tabs = (n: number) =>
  Array.from({ length: n }, (_, i) => ({ url: `u${i}`, label: `t${i}`, committedAt: "2026-01-01T00:00:00Z" }));

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
