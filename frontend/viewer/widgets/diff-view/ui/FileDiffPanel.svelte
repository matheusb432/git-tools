<script lang="ts">
  import { untrack } from "svelte";
  import { AlertTriangle, FileCode2 } from "@lucide/svelte";
  import { getRows, pagesForRange, putRows, rowPageKey, type RowPageCache } from "@/entities/diff-tab";
  import { cn } from "@/shared/lib/utils";
  import { fileRows, type SplitRow, type UnifiedRow } from "@/shared/api";
  import * as Alert from "@/shared/ui/alert";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import { TEST_IDS } from "@/shared/testids";
  import { activePaneKey, type DiffLayout } from "../model/diff-view";
  import { panePageErrorEntries, retryablePanePageRequests, siblingLayout } from "../model/row-render";
  import {
    ESTIMATED_ROW_HEIGHT_PX,
    FILE_PANEL_HEADER_HEIGHT_PX,
    estimateRowCount,
  } from "../model/panel-size-estimate";
  import { ROW_WINDOW_OVERSCAN_PX, visibleRowWindow } from "../model/row-window";
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
    /** This panel's top offset within the outer scroll content. */
    readonly panelTop: number;
    /** Live scroll offset of the single outer scroll surface. */
    readonly outerScrollTop: number;
    /** Live height of the outer viewport. */
    readonly outerViewportHeight: number;
    /** Scroll the outer list so this panel's top aligns to the viewport top. */
    readonly onScrollToTop: () => void;
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
    panelTop,
    outerScrollTop,
    outerViewportHeight,
    onScrollToTop,
    rowCache,
    updateRowCache,
  }: Props = $props();

  let expandedLongRows = $state.raw<ReadonlySet<string>>(new Set<string>());
  let loadingKeys = $state.raw<RowPageLoadingSet>(new Set<string>());
  let pageErrors = $state.raw<RowPageErrorMap>(new Map<string, string>());

  const paneKey = $derived(activePaneKey({ tabId, fileIdx, layout, full }));
  const firstPageCacheKey = $derived(rowPageKey({ tabId, fileIdx, layout, full, pageStart: 0, pageSize: PAGE_SIZE }));

  const firstPage = $derived(getRows(rowCache, firstPageCacheKey));
  const totalRows = $derived(firstPage?.total ?? 0);
  // Before the first page loads, size the panel from metadata so the outer
  // scrollbar is correct and measurement only refines it.
  const windowTotalRows = $derived(firstPage?.total ?? estimateRowCount({ added, removed, layout }));
  const activePageErrors = $derived(panePageErrorEntries(pageErrors, paneKey));
  const hasFirstPageError = $derived(pageErrors.has(firstPageCacheKey));
  const hasAnyError = $derived(activePageErrors.length > 0);

  const rowWindow = $derived(
    expanded
      ? visibleRowWindow({
          outerScrollTop,
          outerViewportHeight,
          panelTop,
          headerHeight: FILE_PANEL_HEADER_HEIGHT_PX,
          totalRows: windowTotalRows,
          rowHeight: ESTIMATED_ROW_HEIGHT_PX,
          overscanPx: ROW_WINDOW_OVERSCAN_PX,
        })
      : null,
  );

  // A primitive identity for the slice so the page-request effect fires only
  // when the quantized range actually changes, not on every scroll frame
  // (visibleRowWindow returns a fresh object each call).
  const rowRequestKey = $derived(rowWindow === null ? null : `${rowWindow.start}:${rowWindow.end}`);

  const beforeSpacerPx = $derived((rowWindow?.start ?? 0) * ESTIMATED_ROW_HEIGHT_PX);
  const afterSpacerPx = $derived(
    Math.max(0, windowTotalRows - (rowWindow?.end ?? -1) - 1) * ESTIMATED_ROW_HEIGHT_PX,
  );
  const reservedSpacerPx = $derived(windowTotalRows * ESTIMATED_ROW_HEIGHT_PX);

  const rowsByIndex = $derived.by(() => {
    const rows = new Map<number, UnifiedRow | SplitRow>();
    for (let pageStart = 0; pageStart < totalRows; pageStart += PAGE_SIZE) {
      const page = getRows(rowCache, rowPageKey({ tabId, fileIdx, layout, full, pageStart, pageSize: PAGE_SIZE }));
      if (page === undefined) continue;
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
    // The panel grows/shrinks; the outer virtualizer's ResizeObserver corrects
    // the measured height automatically — no manual remeasure needed.
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

  function retryErrors(): void {
    for (const page of retryablePanePageRequests(pageErrors, paneKey)) {
      void requestPage(page.pageStart, page.pageSize);
    }
  }

  function resetPaneState(_paneKey: string): void {
    expandedLongRows = new Set<string>();
    loadingKeys = new Set<string>();
    pageErrors = new Map<string, string>();
  }

  $effect(() => {
    resetPaneState(paneKey);
  });

  // Load only the pages the shared outer viewport currently reveals for this
  // panel; far-off panels resolve `rowWindow` to null and fetch nothing. Keyed
  // on the primitive range so it fires per slice change, not per scroll frame.
  $effect(() => {
    if (rowRequestKey === null) return;
    const window = untrack(() => rowWindow);
    if (window === null) return;
    untrack(() => requestVisibleRange(window.start, window.end));
  });
</script>

<section
  data-testid={TEST_IDS.diffView.filePanel}
  class={cn(
    "file rounded-md border bg-surface font-mono",
    selected ? "border-accent/45" : "border-border",
  )}
>
  <div
    class="sticky top-0 z-10 flex items-center gap-2 rounded-t-md border-b border-border bg-muted px-3 py-2 text-[12.5px]"
  >
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
      <Button variant="ghost" size="xs" onclick={onScrollToTop}>Top</Button>
    {/if}
    <Button variant="outline" size="xs" onclick={onToggleExpanded}>
      {expanded ? "Collapse" : "Expand"}
    </Button>
  </div>

  {#if expanded}
    <div class="overflow-hidden rounded-b-md">
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

      {#if hasFirstPageError}
        <!-- The alert above already explains the first-page failure. -->
      {:else if firstPage !== undefined && totalRows === 0}
        <div class="px-4 py-5 text-sm text-foreground-muted">No diff rows are available for this file.</div>
      {:else if rowWindow === null}
        <div style={`height: ${reservedSpacerPx}px;`} aria-hidden="true"></div>
      {:else}
        <div style={`height: ${beforeSpacerPx}px;`} aria-hidden="true"></div>
        <RowWindow
          {layout}
          start={rowWindow.start}
          end={rowWindow.end}
          {rowsByIndex}
          {expandedLongRows}
          onToggleLongRow={toggleLongRow}
        />
        <div style={`height: ${afterSpacerPx}px;`} aria-hidden="true"></div>
      {/if}
    </div>
  {/if}
</section>
