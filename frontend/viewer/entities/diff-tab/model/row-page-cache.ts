import type { RowsPage } from "@/shared/api";

export type RowPageKey = string;
export const ROW_PAGE_CACHE_MAX_PAGES = 8;

export type RowPageKeyParts = {
  readonly tabId: number;
  readonly fileIdx: number;
  readonly layout: "unified" | "split";
  readonly full: boolean;
  readonly pageStart: number;
  readonly pageSize: number;
};

export type RowPageCache = ReadonlyMap<RowPageKey, RowsPage>;

export type RowPageRequest = {
  readonly pageStart: number;
  readonly pageSize: number;
};

export type VisibleRowRange = {
  readonly start: number;
  readonly end: number;
  readonly pageSize: number;
};

export function rowPageKey(parts: RowPageKeyParts): RowPageKey {
  return `${parts.tabId}:${parts.fileIdx}:${parts.layout}:${parts.full ? "full" : "compact"}:${parts.pageStart}:${parts.pageSize}`;
}

export function pagesForRange({ start, end, pageSize }: VisibleRowRange): readonly RowPageRequest[] {
  const normalizedPageSize = Math.max(1, Math.trunc(pageSize));
  const normalizedStart = Math.max(0, Math.trunc(start));
  const normalizedEnd = Math.max(normalizedStart, Math.trunc(end));
  const firstPageStart = Math.floor(normalizedStart / normalizedPageSize) * normalizedPageSize;
  const lastPageStart = Math.floor(normalizedEnd / normalizedPageSize) * normalizedPageSize;
  if (firstPageStart === lastPageStart) {
    return [{ pageStart: firstPageStart, pageSize: normalizedPageSize }];
  }

  return [
    { pageStart: firstPageStart, pageSize: normalizedPageSize },
    { pageStart: Math.min(firstPageStart + normalizedPageSize, lastPageStart), pageSize: normalizedPageSize },
  ];
}

export function emptyRowPageCache(): RowPageCache {
  return new Map<RowPageKey, RowsPage>();
}

export function getRows(cache: RowPageCache, key: RowPageKey): RowsPage | undefined {
  return cache.get(key);
}

export function putRows(cache: RowPageCache, key: RowPageKey, rows: RowsPage): RowPageCache {
  const next = new Map(cache);
  next.delete(key);
  next.set(key, rows);
  while (next.size > ROW_PAGE_CACHE_MAX_PAGES) {
    const oldest = next.keys().next().value;
    if (oldest === undefined) break;
    next.delete(oldest);
  }
  return next;
}

export function invalidateTabRows(cache: RowPageCache, tabId: number): RowPageCache {
  const prefix = `${tabId}:`;
  const next = new Map<RowPageKey, RowsPage>();
  for (const [key, value] of cache) {
    if (!key.startsWith(prefix)) next.set(key, value);
  }
  return next;
}
