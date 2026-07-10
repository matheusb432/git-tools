<script lang="ts">
  import type { UnifiedRow } from "@/shared/api";
  import { cn } from "@/shared/lib/utils";
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
        return "";
      case "context":
        return " ";
    }
  }

  function lineNumber(value: number | null): string {
    return value === null ? "" : `${value}`;
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
    class={cn(
      "grid grid-cols-[44px_44px_minmax(0,1fr)] items-start font-mono text-[14px] leading-[1.6]",
      row.kind === "add" && "bg-add-muted text-add",
      row.kind === "del" && "bg-delete-muted text-delete",
      row.kind === "hunk" && "bg-sunk text-foreground-dim",
      row.kind === "meta" && "opacity-60",
    )}
  >
    <div class="px-2 text-right text-[12px] text-foreground-dim select-none">{lineNumber(row.old_no)}</div>
    <div class="px-2 text-right text-[12px] text-foreground-dim select-none">{lineNumber(row.new_no)}</div>

    <div class="min-w-0 px-3">
      {#if row.long_len !== null && !expanded}
        <div class="flex min-w-0 items-start gap-2">
          <span class="min-w-0 flex-1 truncate" title={`${marker(row)}${row.text}`}>{marker(row)}{row.text}</span>
          <Button
            variant="ghost"
            size="xs"
            class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
            onclick={() => onToggleLongRow(longRowKey)}
          >
            Show full
          </Button>
        </div>
      {:else}
        <div class="flex items-start gap-2">
          <span class="min-w-0 flex-1 break-words whitespace-pre-wrap">{marker(row)}{row.text}</span>
          {#if row.long_len !== null}
            <Button
              variant="ghost"
              size="xs"
              class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
              onclick={() => onToggleLongRow(longRowKey)}
            >
              Collapse
            </Button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/each}
