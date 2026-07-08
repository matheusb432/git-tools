<script lang="ts">
  import { untrack } from "svelte";
  import { AlertTriangle, FileCode2 } from "@lucide/svelte";
  import { getRows, pagesForRange, putRows, rowPageKey, type RowPageCache } from "@/entities/diff-tab";
  import { cn } from "@/shared/lib/utils";
  import { fileRows, type SplitRow, type UnifiedRow } from "@/shared/api";
  import * as Alert from "@/shared/ui/alert";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import { Skeleton } from "@/shared/ui/skeleton";
  import { TEST_IDS } from "@/shared/testids";
  import { activePaneKey, type DiffLayout } from "../model/diff-view";
  import { filePanelBodyState, panePageErrorEntries, retryablePanePageRequests, siblingLayout } from "../model/row-render";
  import RowWindow from "./RowWindow.svelte";

  const PAGE_SIZE = 80;

  type UpdateRowCache = (updater: (cache: RowPageCache) => RowPageCache) => void;

  type Props = {
    readonly tabId: number;
    readonly fileIdx: number;
    readonly path: string;
    readonly status: string;
    readonly added: number;
    readonly removed: number;
    readonly hasFull: boolean;
    readonly commits: readonly string[];
    readonly layout: DiffLayout;
    readonly full: boolean;
    readonly selected: boolean;
    readonly rowCache: RowPageCache;
    readonly updateRowCache: UpdateRowCache;
  };

  type RowPageErrorMap = ReadonlyMap<string, string>;
  type RowPageLoadingSet = ReadonlySet<string>;

  let {
    tabId,
    fileIdx,
    path,
    status,
    added,
    removed,
    hasFull,
    commits,
    layout,
    full,
    selected,
    rowCache,
    updateRowCache,
  }: Props = $props();

  let expandedLongRows = $state.raw<ReadonlySet<string>>(new Set<string>());
  let loadingKeys = $state.raw<RowPageLoadingSet>(new Set<string>());
  let pageErrors = $state.raw<RowPageErrorMap>(new Map<string, string>());
  let rowWindowHandle = $state<{ readonly scrollToIndex: (index: number) => void; readonly measure: () => void } | null>(null);

  const paneKey = $derived(activePaneKey({ tabId, fileIdx, layout, full }));
  const panePrefix = $derived(`${paneKey}:`);
  const firstPageCacheKey = $derived(
    rowPageKey({ tabId, fileIdx, layout, full, pageStart: 0, pageSize: PAGE_SIZE }),
  );

  const firstPage = $derived(getRows(rowCache, firstPageCacheKey));
  const totalRows = $derived(firstPage?.total ?? 0);
  const activePageErrors = $derived(panePageErrorEntries(pageErrors, paneKey));
  const hasFirstPageError = $derived(pageErrors.has(firstPageCacheKey));
  const isInitialLoading = $derived(!hasFirstPageError && firstPage === undefined);
  const hasAnyError = $derived(activePageErrors.length > 0);
  const bodyState = $derived(filePanelBodyState({ isInitialLoading, hasFirstPageError, totalRows }));

  const rowsByIndex = $derived.by(() => {
    const rows = new Map<number, UnifiedRow | SplitRow>();
    for (const [cacheKey, page] of rowCache) {
      if (!cacheKey.startsWith(panePrefix)) continue;
      const parts = cacheKey.split(":");
      const pageStart = Number(parts.at(-2));
      if (!Number.isFinite(pageStart)) continue;
      page.rows.forEach((row, offset) => {
        rows.set(pageStart + offset, row);
      });
    }
    return rows;
  });

  function setLoading(key: string, active: boolean): void {
    const next = new Set(loadingKeys);
    if (active) next.add(key);
    else next.delete(key);
    loadingKeys = next;
  }

  function clearError(key: string): void {
    if (!pageErrors.has(key)) return;
    const next = new Map(pageErrors);
    next.delete(key);
    pageErrors = next;
  }

  function setError(key: string, message: string): void {
    const next = new Map(pageErrors);
    next.set(key, message);
    pageErrors = next;
  }

  function toggleLongRow(rowKey: string): void {
    const next = new Set(expandedLongRows);
    if (next.has(rowKey)) next.delete(rowKey);
    else next.add(rowKey);
    expandedLongRows = next;
    rowWindowHandle?.measure();
  }

  async function requestPage(pageStart: number, pageSize: number): Promise<void> {
    const key = rowPageKey({ tabId, fileIdx, layout, full, pageStart, pageSize });
    if (getRows(rowCache, key) !== undefined || loadingKeys.has(key)) return;

    clearError(key);
    setLoading(key, true);

    try {
      const page = await fileRows({ tabId, fileIdx, layout, full, start: pageStart, count: pageSize });
      if (pageStart === 0) {
        const sibling = siblingLayout(layout);
        const siblingKey = rowPageKey({ tabId, fileIdx, layout: sibling, full, pageStart, pageSize });
        if (getRows(rowCache, siblingKey) === undefined) {
          try {
            const siblingPage = await fileRows({ tabId, fileIdx, layout: sibling, full, start: pageStart, count: pageSize });
            updateRowCache((cache) => putRows(putRows(cache, key, page), siblingKey, siblingPage));
            return;
          } catch {
            // Background prefetch must never hide the currently requested page.
          }
        }
      }
      updateRowCache((cache) => putRows(cache, key, page));
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unable to load diff rows.";
      setError(key, message);
    } finally {
      setLoading(key, false);
    }
  }

  function requestVisibleRange(start: number, end: number): void {
    for (const page of pagesForRange({ start, end, pageSize: PAGE_SIZE })) {
      void requestPage(page.pageStart, page.pageSize);
    }
  }

  function handleVisibleRange(range: { readonly start: number; readonly end: number }): void {
    requestVisibleRange(range.start, range.end);
  }

  function retryErrors(): void {
    for (const page of retryablePanePageRequests(pageErrors, paneKey)) {
      void requestPage(page.pageStart, page.pageSize);
    }
  }

  function scrollToTop(): void {
    rowWindowHandle?.scrollToIndex(0);
  }

  $effect(() => {
    paneKey;
    expandedLongRows = new Set<string>();
    loadingKeys = new Set<string>();
    pageErrors = new Map<string, string>();
  });

  $effect(() => {
    paneKey;
    if (firstPage !== undefined) return;
    untrack(() => {
      if (!loadingKeys.has(firstPageCacheKey)) {
        void requestPage(0, PAGE_SIZE);
      }
    });
  });
</script>

<section
  data-testid={TEST_IDS.diffView.filePanel}
  class={cn(
    "rounded-lg border",
    selected ? "border-accent/40 bg-surface/65" : "border-border bg-surface/40",
  )}
>
  <div class="flex flex-wrap items-start justify-between gap-3 border-b border-border/80 px-4 py-4">
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <FileCode2 class="text-foreground-muted" />
        <h3 class="truncate text-sm font-semibold" title={path}>{path}</h3>
      </div>
      <div class="mt-2 flex flex-wrap items-center gap-2 text-xs text-foreground-muted">
        <Badge variant="outline">{status}</Badge>
        <span class="font-mono text-emerald-300">+{added}</span>
        <span class="font-mono text-rose-300">-{removed}</span>
        {#if hasFull}
          <Badge variant="secondary">full rows ready</Badge>
        {/if}
        {#if commits.length > 0}
          <span class="font-mono">{commits.map((sha) => sha.slice(0, 7)).join(", ")}</span>
        {/if}
      </div>
    </div>

    <Button variant="ghost" size="sm" onclick={scrollToTop}>Top</Button>
  </div>

  {#if hasAnyError}
    <div class="px-4 pt-4">
        <Alert.Root variant="destructive">
        <AlertTriangle />
        <Alert.Title>Could not load some diff rows</Alert.Title>
        <Alert.Description>{activePageErrors[0]?.[1] ?? "Unable to load diff rows."}</Alert.Description>
        <Alert.Action>
          <Button variant="destructive" size="sm" onclick={retryErrors}>Retry</Button>
        </Alert.Action>
      </Alert.Root>
    </div>
  {/if}

  {#if bodyState === "loading"}
    <div class="space-y-2 px-4 py-4">
      <Skeleton class="h-[22px] w-full rounded-md" />
      <Skeleton class="h-[22px] w-full rounded-md" />
      <Skeleton class="h-[22px] w-[92%] rounded-md" />
      <Skeleton class="h-[22px] w-[86%] rounded-md" />
    </div>
  {:else if bodyState === "empty"}
    <div class="px-4 py-5 text-sm text-foreground-muted">No diff rows are available for this file.</div>
  {:else if bodyState === "rows"}
    <div class="h-[200px] min-h-0">
      <RowWindow
        bind:handle={rowWindowHandle}
        {layout}
        total={totalRows}
        {rowsByIndex}
        expandedLongRows={expandedLongRows}
        onToggleLongRow={toggleLongRow}
        onRange={handleVisibleRange}
      />
    </div>
  {/if}
</section>
