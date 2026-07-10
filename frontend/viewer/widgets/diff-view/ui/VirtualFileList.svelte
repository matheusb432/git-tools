<script lang="ts">
  import { untrack } from "svelte";
  import { createVirtualizer } from "@tanstack/svelte-virtual";
  import type { RowPageCache } from "@/entities/diff-tab";
  import type { DiffLayout, FilePanelFoldState } from "../model/diff-view";
  import {
    FILE_PANEL_ESTIMATED_HEIGHT,
    FILE_PANEL_OVERSCAN,
    FILE_PANEL_WINDOW_TARGET,
    filePanelDomKey,
    filePanelOverscanForViewport,
  } from "../model/file-panel-window";
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
    readonly selectedFileIdx: number | null;
    readonly foldState: FilePanelFoldState;
    readonly layout: DiffLayout;
    readonly full: boolean;
    readonly rowCache: RowPageCache;
    readonly updateRowCache: (updater: (cache: RowPageCache) => RowPageCache) => void;
    readonly expanded: (fileIdx: number) => boolean;
    readonly onToggleExpanded: (fileIdx: number) => void;
    handle?: VirtualFileListHandle | null;
  };

  let {
    tabId,
    files,
    selectedFileIdx,
    layout,
    full,
    rowCache,
    updateRowCache,
    expanded,
    onToggleExpanded,
    handle = $bindable<VirtualFileListHandle | null>(null),
  }: Props = $props();

  let scrollElement = $state<HTMLElement | null>(null);
  let viewportHeight = $state(0);
  const panelOverscan = $derived(
    filePanelOverscanForViewport(
      viewportHeight,
      FILE_PANEL_WINDOW_TARGET,
      FILE_PANEL_OVERSCAN,
      FILE_PANEL_ESTIMATED_HEIGHT,
    ),
  );

  const virtualizer = createVirtualizer<HTMLElement, HTMLElement>({
    getScrollElement: () => scrollElement,
    count: 0,
    estimateSize: () => FILE_PANEL_ESTIMATED_HEIGHT,
    overscan: FILE_PANEL_OVERSCAN,
    getItemKey: (index) => files[index]?.fileIdx ?? index,
    onChange: () => undefined,
  });

  function indexForFile(fileIdx: number): number {
    return files.findIndex((file) => file.fileIdx === fileIdx);
  }

  $effect(() => {
    untrack(() => $virtualizer).setOptions({
      count: files.length,
      estimateSize: () => FILE_PANEL_ESTIMATED_HEIGHT,
      overscan: panelOverscan,
      getItemKey: (index) => files[index]?.fileIdx ?? index,
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

  function measurePanel(node: HTMLElement): void {
    $virtualizer.measureElement(node);
  }
</script>

<section
  data-testid={TEST_IDS.diffView.fileList}
  bind:this={scrollElement}
  bind:clientHeight={viewportHeight}
  class="main min-h-0 overflow-auto"
>
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
              selected={selectedFileIdx === file.fileIdx}
              expanded={expanded(file.fileIdx)}
              onToggleExpanded={() => onToggleExpanded(file.fileIdx)}
              {rowCache}
              {updateRowCache}
            />
          </div>
        </section>
      {/if}
    {/each}
  </div>
</section>
