import { createQueries, queryOptions, useQueryClient, type QueryClient } from "@tanstack/svelte-query";
import { derived, get, readable, toStore } from "svelte/store";
import { planRowPages } from "../model/row-page-plan";
import { rowsPageSchema, useApi, type ApiClient, type RowsPage, type SplitRow, type UnifiedRow } from "@/shared/api";

export type RowPageIdentity = {
  readonly tabId: number;
  readonly fileIdx: number;
  readonly layout: "unified" | "split";
  readonly density: "compact" | "full";
  readonly start: number;
  readonly count: number;
};

export function rowPagePrefix(identity: Omit<RowPageIdentity, "start" | "count">) {
  return [
    "viewer",
    "tab",
    identity.tabId,
    "file",
    identity.fileIdx,
    "rows",
    identity.layout,
    identity.density,
  ] as const;
}

export function rowPageKey(identity: RowPageIdentity) {
  return [...rowPagePrefix(identity), identity.start, identity.count] as const;
}

export function rowPageOptions(api: ApiClient, identity: RowPageIdentity) {
  return queryOptions({
    queryKey: rowPageKey(identity),
    queryFn: ({ signal }) =>
      api.query(
        "file_rows",
        {
          tabId: identity.tabId,
          fileIdx: identity.fileIdx,
          layout: identity.layout,
          full: identity.density === "full",
          start: identity.start,
          count: identity.count,
        },
        rowsPageSchema,
        signal,
      ),
    staleTime: Infinity,
    gcTime: 30_000,
  });
}

export async function resetTabRowPages(queryClient: QueryClient, tabId: number): Promise<void> {
  await queryClient.resetQueries({
    queryKey: ["viewer", "tab", tabId],
    predicate: (query) => query.queryKey[3] === "file" && query.queryKey[5] === "rows",
  });
}

export type CreateRowPagesInput = Omit<RowPageIdentity, "start" | "count"> & {
  readonly expanded: boolean;
  readonly intersects: boolean;
  readonly pageSize: number;
  readonly rowWindow: (totalRows: number | null) => { readonly start: number; readonly end: number } | null;
};

export type RowPageError = {
  readonly start: number;
  readonly message: string;
  readonly refetch: () => Promise<unknown>;
};

export type RowPages = {
  readonly firstPage: RowsPage | null;
  readonly totalRows: number | null;
  readonly rowsByIndex: ReadonlyMap<number, UnifiedRow | SplitRow>;
  readonly pendingPageStarts: ReadonlySet<number>;
  readonly errors: readonly RowPageError[];
};

function errorMessage(error: Error | null): string {
  return error?.message ?? "Unable to load diff rows.";
}

export function createRowPages(input: () => CreateRowPagesInput) {
  const api = useApi();
  const queryClient = useQueryClient();
  const currentInput = toStore(input);
  const cacheEvents = readable(0, (set) => {
    let version = 0;
    return queryClient.getQueryCache().subscribe((event) => {
      if (event.type !== "updated") return;
      const current = get(currentInput);
      const key = event.query.queryKey;
      if (
        key[0] === "viewer" &&
        key[1] === "tab" &&
        key[2] === current.tabId &&
        key[3] === "file" &&
        key[4] === current.fileIdx &&
        key[5] === "rows" &&
        key[6] === current.layout &&
        key[7] === current.density
      ) {
        set((version += 1));
      }
    });
  });
  const identities = derived([currentInput, cacheEvents], ([$current]) => {
    const firstIdentity = { ...$current, start: 0, count: $current.pageSize };
    const firstPage = queryClient.getQueryData<RowsPage>(rowPageKey(firstIdentity));
    const totalRows = firstPage?.total ?? null;
    return planRowPages({
      expanded: $current.expanded,
      intersects: $current.intersects,
      pageSize: $current.pageSize,
      rowWindow: $current.rowWindow(totalRows),
      totalRows,
    }).map((request) => ({
      tabId: $current.tabId,
      fileIdx: $current.fileIdx,
      layout: $current.layout,
      density: $current.density,
      start: request.start,
      count: request.count,
    }));
  });
  const options = derived(identities, ($identities) => $identities.map((identity) => rowPageOptions(api, identity)));
  const queries = createQueries({ queries: options });

  return derived([currentInput, identities, queries], ([$current, $identities, $queries]): RowPages => {
    const rowsByIndex = new Map<number, UnifiedRow | SplitRow>();
    const pendingPageStarts = new Set<number>();
    const errors: RowPageError[] = [];
    let firstPage: RowsPage | null = null;
    let totalRows: number | null = null;

    $queries.forEach((query, index) => {
      const identity = $identities[index];
      if (identity === undefined) return;
      if (query.isPending) {
        const end = identity.start + identity.count;
        for (let pageStart = identity.start; pageStart < end; pageStart += $current.pageSize) {
          pendingPageStarts.add(pageStart);
        }
      }
      if (query.isError) {
        errors.push({ start: identity.start, message: errorMessage(query.error), refetch: () => query.refetch() });
      }
      const page = query.data;
      if (page === undefined) return;
      if (identity.start === 0) {
        firstPage = page;
        totalRows = page.total;
      }
      page.rows.forEach((row, offset) => rowsByIndex.set(identity.start + offset, row));
    });

    return {
      firstPage,
      totalRows,
      rowsByIndex,
      pendingPageStarts,
      errors,
    };
  });
}
