import { expect, test } from "bun:test";
import { planRowPages } from "./row-page-plan";

test("an intersecting expanded zero-estimate panel always bootstraps page zero", () => {
  expect(planRowPages({ expanded: true, intersects: true, pageSize: 80, rowWindow: null, totalRows: null })).toEqual([
    { start: 0, count: 80 },
  ]);
});

test("known visible range requests at most two aligned pages", () => {
  expect(
    planRowPages({
      expanded: true,
      intersects: true,
      pageSize: 80,
      rowWindow: { start: 75, end: 170 },
      totalRows: 500,
    }),
  ).toEqual([
    { start: 0, count: 80 },
    { start: 80, count: 80 },
  ]);
});

test("collapsed or offscreen panels observe no row pages", () => {
  expect(planRowPages({ expanded: false, intersects: true, pageSize: 80, rowWindow: null, totalRows: null })).toEqual(
    [],
  );
  expect(planRowPages({ expanded: true, intersects: false, pageSize: 80, rowWindow: null, totalRows: null })).toEqual(
    [],
  );
});
