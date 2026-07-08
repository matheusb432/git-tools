<script lang="ts">
  import type { SplitCell, SplitRow, Span } from "@/shared/api";
  import { Button } from "@/shared/ui/button";
  import { TEST_IDS } from "@/shared/testids";
  import { splitBaseRowKey, splitEachKey, splitLongRowKey, splitMarker } from "../model/row-render";

  type Props = {
    readonly rows: readonly SplitRow[];
    readonly baseIndex: number;
    readonly expandedLongRows: ReadonlySet<string>;
    readonly onToggleLongRow: (rowKey: string) => void;
  };

  type TextSegment = {
    readonly changed: boolean;
    readonly text: string;
  };

  let { rows, baseIndex, expandedLongRows, onToggleLongRow }: Props = $props();

  function cellSegments(text: string, spans: readonly Span[]): readonly TextSegment[] {
    if (spans.length === 0) return [{ changed: false, text }];

    const segments: TextSegment[] = [];
    let cursor = 0;

    for (const span of spans) {
      if (cursor < span.start) {
        segments.push({ changed: false, text: text.slice(cursor, span.start) });
      }
      segments.push({ changed: true, text: text.slice(span.start, span.end) });
      cursor = span.end;
    }

    if (cursor < text.length) {
      segments.push({ changed: false, text: text.slice(cursor) });
    }

    return segments;
  }

  function isExpanded(key: string, longLength: number | null): boolean {
    return longLength === null || expandedLongRows.has(key);
  }

  function lineNumber(value: number | null): string {
    return value === null ? "" : `${value}`;
  }
</script>

{#each rows as row, index (splitEachKey(baseIndex + index, row))}
  {@const rowIndex = baseIndex + index}
  {@const baseKey = splitBaseRowKey(rowIndex)}
  {#if row.kind === "meta" || row.kind === "hunk"}
    <div
      data-testid={TEST_IDS.diffView.row}
      class="grid grid-cols-[28px_minmax(0,1fr)] items-start gap-x-3 border-b border-border/70 px-3 py-1.5 font-mono text-[12.5px] leading-5"
    >
      <div class="pt-0.5 text-center text-sm font-semibold text-foreground-muted">@</div>
      <div class="whitespace-pre-wrap break-words text-foreground">{row.text}</div>
    </div>
  {:else if row.kind === "context"}
    {@const expanded = isExpanded(baseKey, row.long_len)}
    <div
      data-testid={TEST_IDS.diffView.row}
      class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-px border-b border-border/70 bg-border/70"
    >
      {#each [row.old_no, row.new_no] as no, sideIndex (`${baseKey}:${sideIndex}`)}
        <div class="grid min-h-[22px] grid-cols-[28px_56px_88px_minmax(0,1fr)] gap-x-3 bg-surface px-3 py-1.5 font-mono text-[12.5px] leading-5">
          <div class="pt-0.5 text-center text-sm font-semibold text-foreground-muted"> </div>
          <div class="truncate text-right text-foreground-muted">{lineNumber(no)}</div>
          <div class="truncate text-xs text-foreground-muted"></div>
          <div class="min-w-0">
            {#if row.long_len !== null && !expanded}
              <div class="flex min-w-0 items-start gap-2">
                <span class="min-w-0 flex-1 truncate" title={row.text}>{row.text}</span>
                <Button variant="ghost" size="sm" class="h-6 px-2 text-[11px]" onclick={() => onToggleLongRow(baseKey)}>
                  Show full
                </Button>
              </div>
            {:else}
              <div class="flex items-start gap-2">
                <span class="min-w-0 flex-1 whitespace-pre-wrap break-words text-foreground">{row.text}</span>
                {#if row.long_len !== null}
                  <Button variant="ghost" size="sm" class="h-6 px-2 text-[11px]" onclick={() => onToggleLongRow(baseKey)}>
                    Collapse
                  </Button>
                {/if}
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div
      data-testid={TEST_IDS.diffView.row}
      class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-px border-b border-border/70 bg-border/70"
    >
      {#each [{ side: "old", cell: row.old }, { side: "new", cell: row.new }] as sideRow (`${baseKey}:${sideRow.side}`)}
        {@const longKey = splitLongRowKey(rowIndex, sideRow.side)}
        {@const expanded = isExpanded(longKey, sideRow.cell?.long_len ?? null)}
        <div class="grid min-h-[22px] grid-cols-[28px_56px_88px_minmax(0,1fr)] gap-x-3 bg-surface px-3 py-1.5 font-mono text-[12.5px] leading-5">
          <div class="pt-0.5 text-center text-sm font-semibold text-foreground-muted">{splitMarker(sideRow.side, sideRow.cell)}</div>
          <div class="truncate text-right text-foreground-muted">{lineNumber(sideRow.cell?.no ?? null)}</div>
          <div class="truncate text-xs text-foreground-muted" title={sideRow.cell?.owner ?? ""}>{sideRow.cell?.owner ?? ""}</div>

          <div class="min-w-0">
            {#if sideRow.cell === null}
              <div class="min-h-[22px]"></div>
            {:else if sideRow.cell.long_len !== null && !expanded}
              <div class="flex min-w-0 items-start gap-2">
                <span class="min-w-0 flex-1 truncate" title={sideRow.cell.text}>{sideRow.cell.text}</span>
                <Button variant="ghost" size="sm" class="h-6 px-2 text-[11px]" onclick={() => onToggleLongRow(longKey)}>
                  Show full
                </Button>
              </div>
            {:else}
              <div class="flex items-start gap-2">
                <span class="min-w-0 flex-1 whitespace-pre-wrap break-words text-foreground">
                  {#each cellSegments(sideRow.cell.text, sideRow.cell.spans) as segment (`${longKey}:${segment.text}:${segment.changed}`)}
                    {#if segment.changed}
                      <mark class="rounded-sm bg-accent/20 px-0.5 text-foreground">{segment.text}</mark>
                    {:else}
                      {segment.text}
                    {/if}
                  {/each}
                </span>
                {#if sideRow.cell.long_len !== null}
                  <Button variant="ghost" size="sm" class="h-6 px-2 text-[11px]" onclick={() => onToggleLongRow(longKey)}>
                    Collapse
                  </Button>
                {/if}
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
{/each}
