<script lang="ts">
  import type { UnifiedRow } from "@/shared/api";
  import { Button } from "@/shared/ui/button";
  import { TEST_IDS } from "@/shared/testids";
  import { unifiedEachKey, unifiedLongRowKey } from "../model/row-render";

  type Props = {
    readonly rows: readonly UnifiedRow[];
    readonly baseIndex: number;
    readonly expandedLongRows: ReadonlySet<string>;
    readonly onToggleLongRow: (rowKey: string) => void;
  };

  let { rows, baseIndex, expandedLongRows, onToggleLongRow }: Props = $props();

  function marker(row: UnifiedRow): string {
    switch (row.kind) {
      case "add":
        return "+";
      case "del":
        return "-";
      case "meta":
      case "hunk":
        return "@";
      case "context":
        return " ";
    }
  }

  function lineNumber(value: number | null): string {
    return value === null ? "" : `${value}`;
  }

  function ownerLabel(owner: string | null): string {
    return owner ?? "";
  }

  function isExpanded(row: UnifiedRow, index: number): boolean {
    return row.long_len === null || expandedLongRows.has(unifiedLongRowKey(baseIndex + index));
  }
</script>

{#each rows as row, index (unifiedEachKey(baseIndex + index, row))}
  {@const expanded = isExpanded(row, index)}
  {@const longRowKey = unifiedLongRowKey(baseIndex + index)}
  <div
    data-testid={TEST_IDS.diffView.row}
    class="grid grid-cols-[28px_56px_56px_112px_minmax(0,1fr)] items-start gap-x-3 border-b border-border/70 px-3 py-1.5 font-mono text-[12.5px] leading-5"
  >
    <div class="pt-0.5 text-center text-sm font-semibold text-foreground-muted">{marker(row)}</div>
    <div class="truncate text-right text-foreground-muted">{lineNumber(row.old_no)}</div>
    <div class="truncate text-right text-foreground-muted">{lineNumber(row.new_no)}</div>
    <div class="truncate text-xs text-foreground-muted" title={ownerLabel(row.owner)}>{ownerLabel(row.owner)}</div>

    <div class="min-w-0">
      {#if row.long_len !== null && !expanded}
        <div class="flex min-w-0 items-start gap-2">
          <span class="min-w-0 flex-1 truncate" title={row.text}>{row.text}</span>
          <Button variant="ghost" size="sm" class="h-6 px-2 text-[11px]" onclick={() => onToggleLongRow(longRowKey)}>
            Show full
          </Button>
        </div>
      {:else}
        <div class="flex items-start gap-2">
          <span class="min-w-0 flex-1 break-words whitespace-pre-wrap text-foreground">{row.text}</span>
          {#if row.long_len !== null}
            <Button variant="ghost" size="sm" class="h-6 px-2 text-[11px]" onclick={() => onToggleLongRow(longRowKey)}>
              Collapse
            </Button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/each}
