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
  import {
    filePanelBodyState,
    panePageErrorEntries,
    retryablePanePageRequests,
    siblingLayout,
  } from "../model/row-render";
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
    readonly expanded: boolean;
    readonly onToggleExpanded: () => void;
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
    expanded,
    onToggleExpanded,
    rowCache,
    updateRowCache,
  }: Props = $props();

  let expandedLongRows = $state.raw<ReadonlySet<string>>(new Set<string>());
  let loadingKeys = $state.raw<RowPageLoadingSet>(new Set<string>());
  let pageErrors = $state.raw<RowPageErrorMap>(new Map<string, string>());
  let rowWindowHandle = $state<{
    readonly scrollToIndex: (index: number) => void;
    readonly measure: () => void;
  } | null>(null);

  const paneKey = $derived(activePaneKey({ tabId, fileIdx, layout, full }));
  const panePrefix = $derived(`${paneKey}:`);
  const firstPageCacheKey = $derived(rowPageKey({ tabId, fileIdx, layout, full, pageStart: 0, pageSize: PAGE_SIZE }));

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

  async function prefetchSiblingFirstPage(pageStart: number, pageSize: number): Promise<void> {
    const sibling = siblingLayout(layout);
    const siblingKey = rowPageKey({ tabId, fileIdx, layout: sibling, full, pageStart, pageSize });
    if (getRows(rowCache, siblingKey) !== undefined) return;

    try {
      const siblingPage = await fileRows({ tabId, fileIdx, layout: sibling, full, start: pageStart, count: pageSize });
      updateRowCache((cache) => putRows(cache, siblingKey, siblingPage));
    } catch {
      // Background prefetch must never block or replace the visible pane.
    }
  }

  async function requestPage(pageStart: number, pageSize: number): Promise<void> {
    const key = rowPageKey({ tabId, fileIdx, layout, full, pageStart, pageSize });
    if (getRows(rowCache, key) !== undefined || loadingKeys.has(key)) return;

    clearError(key);
    setLoading(key, true);

    try {
      const page = await fileRows({ tabId, fileIdx, layout, full, start: pageStart, count: pageSize });
      updateRowCache((cache) => putRows(cache, key, page));
      if (pageStart === 0) {
        void prefetchSiblingFirstPage(pageStart, pageSize);
      }
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

  function handleTopClick(): void {
    scrollToTop();
  }

  function handleToggleClick(): void {
    onToggleExpanded();
  }

  function resetPaneState(_paneKey: string): void {
    expandedLongRows = new Set<string>();
    loadingKeys = new Set<string>();
    pageErrors = new Map<string, string>();
  }

  function ensureFirstPageLoaded(_paneKey: string): void {
    if (!expanded) return;
    if (firstPage !== undefined) return;
    untrack(() => {
      if (!loadingKeys.has(firstPageCacheKey)) {
        void requestPage(0, PAGE_SIZE);
      }
    });
  }

  $effect(() => {
    resetPaneState(paneKey);
  });

  $effect(() => {
    ensureFirstPageLoaded(paneKey);
  });
</script>

<section
  data-testid={TEST_IDS.diffView.filePanel}
  class={cn(
    "file overflow-hidden rounded-md border bg-surface font-mono",
    selected ? "border-accent/45" : "border-border",
  )}
>
  <div class="sticky top-0 z-10 flex items-center gap-2 border-b border-border bg-muted px-3 py-2 text-[12.5px]">
    <button
      type="button"
      class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 text-left focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none"
      aria-expanded={expanded}
      aria-label={`${expanded ? "Collapse" : "Expand"} ${path}`}
      onclick={onToggleExpanded}
    >
      <FileCode2 class="size-4 shrink-0 text-foreground-muted" />
      <span class="min-w-0 flex-1">
        <span class="block truncate text-[12.5px] text-foreground" title={path}>{path}</span>
        <span class="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-foreground-muted">
          <Badge variant="outline">{status}</Badge>
          <span class="text-add">+{added}</span>
          <span class="text-delete">-{removed}</span>
          {#if hasFull}
            <Badge variant="secondary">full rows ready</Badge>
          {/if}
          {#if commits.length > 0}
            <span class="font-mono">{commits.map((sha) => sha.slice(0, 7)).join(", ")}</span>
          {/if}
        </span>
      </span>
    </button>

    {#if expanded}
      <Button variant="ghost" size="xs" onclick={handleTopClick}>Top</Button>
    {/if}
    <Button variant="outline" size="xs" onclick={handleToggleClick}>
      {expanded ? "Collapse" : "Expand"}
    </Button>
  </div>

  {#if expanded}
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
      <div class="h-[220px] min-h-0">
        <RowWindow
          bind:handle={rowWindowHandle}
          {layout}
          total={totalRows}
          {rowsByIndex}
          {expandedLongRows}
          onToggleLongRow={toggleLongRow}
          onRange={handleVisibleRange}
        />
      </div>
    {/if}
  {/if}
</section>
