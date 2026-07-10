<script lang="ts">
  import type { SplitRow, UnifiedRow } from "@/shared/api";
  import { Skeleton } from "@/shared/ui/skeleton";
  import { TEST_IDS } from "@/shared/testids";
  import SplitRows from "./SplitRows.svelte";
  import UnifiedRows from "./UnifiedRows.svelte";

  type Props = {
    readonly layout: "unified" | "split";
    readonly start: number;
    readonly end: number;
    readonly rowsByIndex: ReadonlyMap<number, UnifiedRow | SplitRow>;
    readonly expandedLongRows: ReadonlySet<string>;
    readonly onToggleLongRow: (rowKey: string) => void;
  };

  let { layout, start, end, rowsByIndex, expandedLongRows, onToggleLongRow }: Props = $props();

  const indexes = $derived.by(() => {
    const list: number[] = [];
    for (let index = start; index <= end; index += 1) list.push(index);
    return list;
  });

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

<div data-testid={TEST_IDS.diffView.rowWindow} class="diff bg-surface">
  {#each indexes as index (index)}
    {#if layout === "unified"}
      {@const row = unifiedRow(index)}
      {#if row !== undefined}
        <UnifiedRows rows={[row]} baseIndex={index} {expandedLongRows} {onToggleLongRow} />
      {:else}
        <div class="px-3 py-1.5">
          <Skeleton class="h-[22px] w-full rounded-md" />
        </div>
      {/if}
    {:else}
      {@const row = splitRow(index)}
      {#if row !== undefined}
        <SplitRows rows={[row]} baseIndex={index} {expandedLongRows} {onToggleLongRow} />
      {:else}
        <div class="px-3 py-1.5">
          <Skeleton class="h-[22px] w-full rounded-md" />
        </div>
      {/if}
    {/if}
  {/each}
</div>
