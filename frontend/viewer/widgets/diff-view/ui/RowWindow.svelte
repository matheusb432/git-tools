<script lang="ts">
  import { untrack } from "svelte";
  import { createVirtualizer } from "@tanstack/svelte-virtual";
  import type { SplitRow, UnifiedRow } from "@/shared/api";
  import { Skeleton } from "@/shared/ui/skeleton";
  import { TEST_IDS } from "@/shared/testids";
  import SplitRows from "./SplitRows.svelte";
  import UnifiedRows from "./UnifiedRows.svelte";

  const ESTIMATED_ROW_HEIGHT = 22;
  const PAGE_SIZE = 80;
  const MAX_DOM_ROWS = 500;
  const OVERSCAN_ROWS = Math.max(2, Math.min(PAGE_SIZE / 8, Math.floor(MAX_DOM_ROWS / 80)));

  type RowWindowHandle = {
    readonly scrollToIndex: (index: number) => void;
    readonly measure: () => void;
  };

  type RowRange = {
    readonly start: number;
    readonly end: number;
  };

  type Props = {
    readonly total: number;
    readonly layout: "unified" | "split";
    readonly rowsByIndex: ReadonlyMap<number, UnifiedRow | SplitRow>;
    readonly expandedLongRows: ReadonlySet<string>;
    readonly onToggleLongRow: (rowKey: string) => void;
    readonly onRange: (range: RowRange) => void;
    handle?: RowWindowHandle | null;
  };

  let {
    total,
    layout,
    rowsByIndex,
    expandedLongRows,
    onToggleLongRow,
    onRange,
    handle = $bindable<RowWindowHandle | null>(null),
  }: Props = $props();

  let scrollElement = $state<HTMLElement | null>(null);

  const virtualizer = createVirtualizer<HTMLElement, HTMLDivElement>({
    getScrollElement: () => scrollElement,
    count: 0,
    estimateSize: () => ESTIMATED_ROW_HEIGHT,
    overscan: OVERSCAN_ROWS,
    getItemKey: (index) => index,
    onChange: () => undefined,
  });

  $effect(() => {
    untrack(() => $virtualizer).setOptions({
      count: total,
      overscan: OVERSCAN_ROWS,
      getItemKey: (index) => `${layout}:${index}`,
      onChange: (instance) => {
        if (instance.range !== null) {
          onRange({ start: instance.range.startIndex, end: instance.range.endIndex });
        }
      },
    });
  });

  $effect(() => {
    const instance = untrack(() => $virtualizer);
    handle = {
      scrollToIndex: (index: number) => {
        instance.scrollToIndex(index, { align: "start" });
      },
      measure: () => {
        instance.measure();
      },
    };
  });

  const virtualItems = $derived($virtualizer.getVirtualItems());
  const totalSize = $derived($virtualizer.getTotalSize());

  function measureRow(node: HTMLDivElement): { update: () => void } {
    $virtualizer.measureElement(node);
    return {
      update() {
        $virtualizer.measureElement(node);
      },
    };
  }

  function unifiedRow(index: number): UnifiedRow | undefined {
    const row = rowsByIndex.get(index);
    if (row === undefined) return undefined;
    if (row.kind === "add" || row.kind === "del") return row;
    if ((row.kind === "meta" || row.kind === "hunk" || row.kind === "context") && "owner" in row) return row;
    return undefined;
  }

  function splitRow(index: number): SplitRow | undefined {
    const row = rowsByIndex.get(index);
    if (row === undefined) return undefined;
    if (row.kind === "pair") return row;
    if ((row.kind === "meta" || row.kind === "hunk") && !("owner" in row)) return row;
    if (row.kind === "context" && !("owner" in row)) return row;
    return undefined;
  }
</script>

<div data-testid={TEST_IDS.diffView.rowWindow} bind:this={scrollElement} class="h-full min-h-0 overflow-auto">
  <div class="relative w-full" style={`height: ${totalSize}px;`}>
    {#each virtualItems as item (item.key)}
      <div
        use:measureRow
        data-index={item.index}
        class="absolute left-0 top-0 w-full"
        style={`transform: translateY(${item.start}px);`}
      >
        {#if layout === "unified"}
          {@const row = unifiedRow(item.index)}
          {#if row !== undefined}
            <UnifiedRows rows={[row]} baseIndex={item.index} {expandedLongRows} {onToggleLongRow} />
          {:else}
            <div class="px-3 py-1.5">
              <Skeleton class="h-[22px] w-full rounded-md" />
            </div>
          {/if}
        {:else}
          {@const row = splitRow(item.index)}
          {#if row !== undefined}
            <SplitRows rows={[row]} baseIndex={item.index} {expandedLongRows} {onToggleLongRow} />
          {:else}
            <div class="px-3 py-1.5">
              <Skeleton class="h-[22px] w-full rounded-md" />
            </div>
          {/if}
        {/if}
      </div>
    {/each}
  </div>
</div>
