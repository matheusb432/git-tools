<script lang="ts">
  import { untrack } from "svelte";
  import { useQueryClient } from "@tanstack/svelte-query";
  import { useSelector } from "@xstate/store-svelte";
  import { AlertTriangle, FileCode2 } from "@lucide/svelte";
  import { createRowPages, rowPageOptions } from "../../api/row-pages.svelte";
  import type { DiffReviewStore } from "../../model/diff-review";
  import { activePaneKey, filePanelIntersectsViewport, type DiffLayout } from "../../model/file-panels";
  import { cn } from "@/shared/lib/utils";
  import { useApi } from "@/shared/api";
  import * as Alert from "@/shared/ui/alert";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import { TEST_IDS } from "@/shared/testids";
  import {
    expandedLongRowKeysForFile,
    expandedLongRowsForPane,
    sameStringSet,
    scopedLongRowKey,
    siblingLayout,
  } from "../../model/row-render";
  import { ESTIMATED_ROW_HEIGHT_PX, FILE_PANEL_HEADER_HEIGHT_PX, estimateRowCount } from "../../model/panel-size";
  import { ROW_WINDOW_OVERSCAN_PX, visibleRowWindow } from "../../model/row-window";
  import RowWindow from "./RowWindow.svelte";

  const PAGE_SIZE = 80;

  type Props = {
    readonly tabId: number;
    readonly reviewStore: DiffReviewStore;
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
    /** Measured or estimated height from the outer file virtualizer. */
    readonly panelHeight: number;
    /** Live scroll offset of the single outer scroll surface. */
    readonly outerScrollTop: number;
    /** Live height of the outer viewport. */
    readonly outerViewportHeight: number;
    /** Scroll the outer list so this panel's top aligns to the viewport top. */
    readonly onScrollToTop: () => void;
  };

  let {
    tabId,
    reviewStore,
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
    panelHeight,
    outerScrollTop,
    outerViewportHeight,
    onScrollToTop,
  }: Props = $props();

  const mountedReviewStore = untrack(() => reviewStore);
  const mountedTabId = untrack(() => tabId);
  const mountedFileIdx = untrack(() => fileIdx);
  const api = useApi();
  const queryClient = useQueryClient();
  const expandedFileLongRows = useSelector(
    mountedReviewStore,
    (snapshot) => expandedLongRowKeysForFile(snapshot.context.expandedLongRows, mountedTabId, mountedFileIdx),
    sameStringSet,
  );
  const paneKey = $derived(activePaneKey({ tabId, fileIdx, layout, full }));
  const expandedLongRows = $derived(expandedLongRowsForPane($expandedFileLongRows, paneKey));
  const density = $derived(full ? "full" : "compact");
  const intersects = $derived(
    filePanelIntersectsViewport({
      panelTop,
      panelHeight,
      outerScrollTop,
      outerViewportHeight,
      overscanPx: ROW_WINDOW_OVERSCAN_PX,
    }),
  );

  function rowWindowForTotal(authoritativeTotal: number | null) {
    if (!expanded || !intersects) return null;
    const totalRows = authoritativeTotal ?? estimateRowCount({ added, removed, layout });
    return visibleRowWindow({
      outerScrollTop,
      outerViewportHeight,
      panelTop,
      headerHeight: FILE_PANEL_HEADER_HEIGHT_PX,
      totalRows,
      rowHeight: ESTIMATED_ROW_HEIGHT_PX,
      overscanPx: ROW_WINDOW_OVERSCAN_PX,
    });
  }

  const rowPages = createRowPages(() => ({
    tabId,
    fileIdx,
    layout,
    density,
    expanded,
    intersects,
    pageSize: PAGE_SIZE,
    rowWindow: rowWindowForTotal,
  }));
  const firstPage = $derived($rowPages.firstPage);
  const totalRows = $derived($rowPages.totalRows ?? 0);
  const windowTotalRows = $derived($rowPages.totalRows ?? estimateRowCount({ added, removed, layout }));
  const rowWindow = $derived(rowWindowForTotal($rowPages.totalRows));
  const activePageErrors = $derived($rowPages.errors);
  const hasFirstPageError = $derived(activePageErrors.some((error) => error.start === 0));
  const hasAnyError = $derived(activePageErrors.length > 0);

  const beforeSpacerPx = $derived((rowWindow?.start ?? 0) * ESTIMATED_ROW_HEIGHT_PX);
  const afterSpacerPx = $derived(Math.max(0, windowTotalRows - (rowWindow?.end ?? -1) - 1) * ESTIMATED_ROW_HEIGHT_PX);
  const reservedSpacerPx = $derived(windowTotalRows * ESTIMATED_ROW_HEIGHT_PX);

  const rowsByIndex = $derived($rowPages.rowsByIndex);

  function toggleLongRow(rowKey: string): void {
    mountedReviewStore.trigger["longRow.toggled"]({ rowKey: scopedLongRowKey(paneKey, rowKey) });
    // The panel grows/shrinks; the outer virtualizer's ResizeObserver corrects
    // the measured height automatically — no manual remeasure needed.
  }

  function retryErrors(): void {
    for (const error of activePageErrors) void error.refetch();
  }

  $effect(() => {
    if (firstPage === null) return;
    void queryClient.prefetchQuery(
      rowPageOptions(api, {
        tabId,
        fileIdx,
        layout: siblingLayout(layout),
        density,
        start: 0,
        count: PAGE_SIZE,
      }),
    );
  });
</script>

<section
  data-testid={TEST_IDS.diffView.filePanel}
  class={cn("file rounded-md border bg-surface font-mono", selected ? "border-accent/45" : "border-border")}
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
            <Alert.Description>{activePageErrors[0]?.message ?? "Unable to load diff rows."}</Alert.Description>
            <Alert.Action>
              <Button variant="destructive" size="sm" onclick={retryErrors}>Retry</Button>
            </Alert.Action>
          </Alert.Root>
        </div>
      {/if}

      {#if hasFirstPageError}
        <!-- The alert above already explains the first-page failure. -->
      {:else if firstPage !== null && totalRows === 0}
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
          pendingPageStarts={$rowPages.pendingPageStarts}
          pageSize={PAGE_SIZE}
          {expandedLongRows}
          onToggleLongRow={toggleLongRow}
        />
        <div style={`height: ${afterSpacerPx}px;`} aria-hidden="true"></div>
      {/if}
    </div>
  {/if}
</section>
