import { expect, test } from "bun:test";
import {
  emptyRowPageCache,
  getRows,
  invalidateTabRows,
  pagesForRange,
  putRows,
  rowPageKey,
  ROW_PAGE_CACHE_MAX_PAGES,
} from "./row-page-cache";

test("row cache key includes tab, file, layout, full, start, and size", () => {
  expect(rowPageKey({ tabId: 7, fileIdx: 2, layout: "split", full: true, pageStart: 160, pageSize: 80 })).toBe(
    "7:2:split:full:160:80",
  );
});

test("visible range expands to bounded row pages", () => {
  expect(pagesForRange({ start: 35, end: 122, pageSize: 80 })).toEqual([
    { pageStart: 0, pageSize: 80 },
    { pageStart: 80, pageSize: 80 },
  ]);
});

test("visible range never expands beyond two row pages", () => {
  expect(pagesForRange({ start: 0, end: 400, pageSize: 80 })).toEqual([
    { pageStart: 0, pageSize: 80 },
    { pageStart: 80, pageSize: 80 },
  ]);
});

test("row cache invalidates one tab without deleting other tabs", () => {
  const cache = putRows(
    putRows(
      emptyRowPageCache(),
      rowPageKey({ tabId: 7, fileIdx: 0, layout: "unified", full: false, pageStart: 0, pageSize: 80 }),
      { total: 1, layout: "unified", rows: [] },
    ),
    rowPageKey({ tabId: 8, fileIdx: 0, layout: "unified", full: false, pageStart: 0, pageSize: 80 }),
    { total: 2, layout: "unified", rows: [] },
  );
  const next = invalidateTabRows(cache, 7);

  expect(
    getRows(next, rowPageKey({ tabId: 7, fileIdx: 0, layout: "unified", full: false, pageStart: 0, pageSize: 80 })),
  ).toBeUndefined();
  expect(
    getRows(next, rowPageKey({ tabId: 8, fileIdx: 0, layout: "unified", full: false, pageStart: 0, pageSize: 80 })),
  ).toEqual({
    total: 2,
    layout: "unified",
    rows: [],
  });
});

test("row cache evicts oldest pages at a fixed budget", () => {
  let cache = emptyRowPageCache();
  for (let fileIdx = 0; fileIdx < ROW_PAGE_CACHE_MAX_PAGES + 3; fileIdx += 1) {
    cache = putRows(
      cache,
      rowPageKey({ tabId: 7, fileIdx, layout: "unified", full: false, pageStart: 0, pageSize: 80 }),
      { total: 1, layout: "unified", rows: [] },
    );
  }

  expect(cache.size).toBe(ROW_PAGE_CACHE_MAX_PAGES);
  expect(
    getRows(cache, rowPageKey({ tabId: 7, fileIdx: 0, layout: "unified", full: false, pageStart: 0, pageSize: 80 })),
  ).toBeUndefined();
  expect(
    getRows(cache, rowPageKey({ tabId: 7, fileIdx: 2, layout: "unified", full: false, pageStart: 0, pageSize: 80 })),
  ).toBeUndefined();
  expect(
    getRows(cache, rowPageKey({ tabId: 7, fileIdx: 3, layout: "unified", full: false, pageStart: 0, pageSize: 80 })),
  ).toEqual({
    total: 1,
    layout: "unified",
    rows: [],
  });
});

test("row cache refreshes an existing page as most recently used", () => {
  let cache = emptyRowPageCache();
  const retainedKey = rowPageKey({ tabId: 7, fileIdx: 0, layout: "unified", full: false, pageStart: 0, pageSize: 80 });
  cache = putRows(cache, retainedKey, { total: 1, layout: "unified", rows: [] });

  for (let fileIdx = 1; fileIdx < ROW_PAGE_CACHE_MAX_PAGES; fileIdx += 1) {
    cache = putRows(
      cache,
      rowPageKey({ tabId: 7, fileIdx, layout: "unified", full: false, pageStart: 0, pageSize: 80 }),
      { total: 1, layout: "unified", rows: [] },
    );
  }

  cache = putRows(cache, retainedKey, { total: 2, layout: "unified", rows: [] });
  cache = putRows(
    cache,
    rowPageKey({
      tabId: 7,
      fileIdx: ROW_PAGE_CACHE_MAX_PAGES,
      layout: "unified",
      full: false,
      pageStart: 0,
      pageSize: 80,
    }),
    { total: 1, layout: "unified", rows: [] },
  );

  expect(getRows(cache, retainedKey)).toEqual({ total: 2, layout: "unified", rows: [] });
  expect(
    getRows(cache, rowPageKey({ tabId: 7, fileIdx: 1, layout: "unified", full: false, pageStart: 0, pageSize: 80 })),
  ).toBeUndefined();
});
