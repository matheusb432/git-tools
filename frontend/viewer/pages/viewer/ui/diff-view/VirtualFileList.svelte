<script lang="ts">
  import { untrack } from "svelte";
  import { useSelector } from "@xstate/store-svelte";
  import { createVirtualizer } from "@tanstack/svelte-virtual";
  import type { DiffReviewStore } from "../../model/diff-review";
  import { filePanelDomKey, filePanelWindowIndexes, type DiffLayout } from "../../model/file-panels";
  import {
    FILE_PANEL_CHROME_PADDING_PX,
    FILE_PANEL_HEADER_HEIGHT_PX,
    estimatePanelHeight,
  } from "../../model/panel-size";
  import { TEST_IDS } from "@/shared/testids";
  import FileDiffPanel from "./FileDiffPanel.svelte";

  export type VisibleFilePanel = {
    readonly fileIdx: number;
    readonly path: string;
    readonly status: string;
    readonly added: number;
    readonly removed: number;
    readonly hasFull: boolean;
    readonly commits: readonly string[];
  };

  export type VirtualFileListHandle = {
    readonly scrollToFile: (fileIdx: number) => void;
    readonly measure: () => void;
  };

  type Props = {
    readonly tabId: number;
    readonly files: readonly VisibleFilePanel[];
    readonly reviewStore: DiffReviewStore;
    readonly layout: DiffLayout;
    readonly full: boolean;
    handle?: VirtualFileListHandle | null;
  };

  let {
    tabId,
    files,
    reviewStore,
    layout,
    full,
    handle = $bindable<VirtualFileListHandle | null>(null),
  }: Props = $props();

  const mountedReviewStore = untrack(() => reviewStore);
  const selectedFileIdx = useSelector(mountedReviewStore, (snapshot) => snapshot.context.selectedFileIdx);
  const collapsedFileIdxs = useSelector(mountedReviewStore, (snapshot) => snapshot.context.collapsedFileIdxs);
  const activeFileIdx = $derived(
    $selectedFileIdx !== null && files.some((file) => file.fileIdx === $selectedFileIdx)
      ? $selectedFileIdx
      : (files[0]?.fileIdx ?? null),
  );

  let scrollElement = $state<HTMLElement | null>(null);

  function estimateSize(index: number): number {
    const file = files[index];
    if (file === undefined) return FILE_PANEL_HEADER_HEIGHT_PX + FILE_PANEL_CHROME_PADDING_PX;
    return estimatePanelHeight({
      added: file.added,
      removed: file.removed,
      layout,
      collapsed: $collapsedFileIdxs.has(file.fileIdx),
    });
  }

  const virtualizer = createVirtualizer<HTMLElement, HTMLElement>({
    getScrollElement: () => scrollElement,
    count: 0,
    estimateSize,
    getItemKey: (index) => files[index]?.fileIdx ?? index,
    rangeExtractor: filePanelWindowIndexes,
    useAnimationFrameWithResizeObserver: true,
    onChange: () => undefined,
  });

  function indexForFile(fileIdx: number): number {
    return files.findIndex((file) => file.fileIdx === fileIdx);
  }

  function scrollPanelToTop(index: number): void {
    untrack(() => $virtualizer).scrollToIndex(index, { align: "start" });
  }

  $effect(() => {
    // Re-read layout/fold state so estimateSize re-estimates unmeasured panels
    // after a layout toggle or fold change.
    void layout;
    void $collapsedFileIdxs;
    untrack(() => $virtualizer).setOptions({
      count: files.length,
      estimateSize,
      getItemKey: (index) => files[index]?.fileIdx ?? index,
      rangeExtractor: filePanelWindowIndexes,
    });
  });

  $effect(() => {
    const instance = untrack(() => $virtualizer);
    handle = {
      scrollToFile: (fileIdx: number) => {
        const index = indexForFile(fileIdx);
        if (index >= 0) instance.scrollToIndex(index, { align: "start" });
      },
      measure: () => instance.measure(),
    };
  });

  const virtualItems = $derived($virtualizer.getVirtualItems());
  const totalSize = $derived($virtualizer.getTotalSize());
  const outerScrollTop = $derived($virtualizer.scrollOffset ?? 0);
  const outerViewportHeight = $derived($virtualizer.scrollRect?.height ?? 0);

  function measurePanel(node: HTMLElement): void {
    $virtualizer.measureElement(node);
  }
</script>

<section data-testid={TEST_IDS.diffView.fileList} bind:this={scrollElement} class="main min-h-0 overflow-auto">
  <div class="relative w-full" style={`height: ${totalSize}px;`}>
    {#each virtualItems as item (item.key)}
      {@const file = files[item.index]}
      {#if file !== undefined}
        <section
          data-testid={TEST_IDS.diffView.filePanelMount}
          id={`diff-file-${file.fileIdx}`}
          class="absolute top-0 left-0 w-full"
          style={`transform: translateY(${item.start}px);`}
          data-file-idx={file.fileIdx}
          data-index={item.index}
          use:measurePanel
        >
          <div class="pb-3" data-file-key={filePanelDomKey(file)}>
            <FileDiffPanel
              {tabId}
              fileIdx={file.fileIdx}
              path={file.path}
              status={file.status}
              added={file.added}
              removed={file.removed}
              hasFull={file.hasFull}
              commits={file.commits}
              {layout}
              {full}
              reviewStore={mountedReviewStore}
              selected={activeFileIdx === file.fileIdx}
              expanded={!$collapsedFileIdxs.has(file.fileIdx)}
              onToggleExpanded={() => mountedReviewStore.trigger["panel.toggled"]({ fileIdx: file.fileIdx })}
              panelTop={item.start}
              panelHeight={item.size}
              {outerScrollTop}
              {outerViewportHeight}
              onScrollToTop={() => scrollPanelToTop(item.index)}
            />
          </div>
        </section>
      {/if}
    {/each}
  </div>
</section>
