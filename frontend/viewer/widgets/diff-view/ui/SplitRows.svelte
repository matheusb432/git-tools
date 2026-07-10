<script lang="ts">
  import type { SplitRow, Span } from "@/shared/api";
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
      class="grid grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)] items-stretch bg-sunk font-mono text-[14px] leading-[1.6] text-foreground-dim"
    >
      <div class="col-span-4 min-w-0 px-3 font-semibold break-words whitespace-pre-wrap">{row.text}</div>
    </div>
  {:else if row.kind === "context"}
    {@const expanded = isExpanded(baseKey, row.long_len)}
    <div
      data-testid={TEST_IDS.diffView.row}
      class="grid grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)] items-stretch font-mono text-[14px] leading-[1.6]"
    >
      <div class="px-2 text-right text-[12px] text-foreground-dim select-none">{lineNumber(row.old_no)}</div>
      <div class="min-w-0 px-3 text-foreground-muted">
        {#if row.long_len !== null && !expanded}
          <div class="flex min-w-0 items-start gap-2">
            <span class="min-w-0 flex-1 truncate" title={row.text}>{row.text}</span>
            <Button
              variant="ghost"
              size="xs"
              class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
              onclick={() => onToggleLongRow(baseKey)}
            >
              Show full
            </Button>
          </div>
        {:else}
          <div class="flex items-start gap-2">
            <span class="min-w-0 flex-1 break-words whitespace-pre-wrap">{row.text}</span>
            {#if row.long_len !== null}
              <Button
                variant="ghost"
                size="xs"
                class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
                onclick={() => onToggleLongRow(baseKey)}
              >
                Collapse
              </Button>
            {/if}
          </div>
        {/if}
      </div>
      <div class="border-l border-border px-2 text-right text-[12px] text-foreground-dim select-none">
        {lineNumber(row.new_no)}
      </div>
      <div class="min-w-0 px-3 text-foreground-muted">
        {#if row.long_len !== null && !expanded}
          <span class="block truncate" title={row.text}>{row.text}</span>
        {:else}
          <span class="block break-words whitespace-pre-wrap">{row.text}</span>
        {/if}
      </div>
    </div>
  {:else}
    {@const oldKey = splitLongRowKey(rowIndex, "old")}
    {@const oldExpanded = isExpanded(oldKey, row.old?.long_len ?? null)}
    {@const newKey = splitLongRowKey(rowIndex, "new")}
    {@const newExpanded = isExpanded(newKey, row.new?.long_len ?? null)}
    <div
      data-testid={TEST_IDS.diffView.row}
      class="grid grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)] items-stretch font-mono text-[14px] leading-[1.6]"
    >
      <div class="px-2 text-right text-[12px] text-foreground-dim select-none">{lineNumber(row.old?.no ?? null)}</div>
      {#if row.old === null}
        <div class="min-h-[22px] bg-sunk"></div>
      {:else}
        <div class="min-w-0 bg-delete-muted px-3 text-delete">
          {#if row.old.long_len !== null && !oldExpanded}
            <div class="flex min-w-0 items-start gap-2">
              <span class="min-w-0 flex-1 truncate" title={`${splitMarker("old", row.old)}${row.old.text}`}
                >{splitMarker("old", row.old)}{row.old.text}</span
              >
              <Button
                variant="ghost"
                size="xs"
                class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
                onclick={() => onToggleLongRow(oldKey)}
              >
                Show full
              </Button>
            </div>
          {:else}
            <div class="flex items-start gap-2">
              <span class="min-w-0 flex-1 break-words whitespace-pre-wrap">
                {splitMarker(
                  "old",
                  row.old,
                )}{#each cellSegments(row.old.text, row.old.spans) as segment (`${oldKey}:${segment.text}:${segment.changed}`)}
                  {#if segment.changed}
                    <mark class="rounded-sm bg-delete/35 px-0.5 text-delete">{segment.text}</mark>
                  {:else}
                    {segment.text}
                  {/if}
                {/each}
              </span>
              {#if row.old.long_len !== null}
                <Button
                  variant="ghost"
                  size="xs"
                  class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
                  onclick={() => onToggleLongRow(oldKey)}
                >
                  Collapse
                </Button>
              {/if}
            </div>
          {/if}
        </div>
      {/if}

      <div class="border-l border-border px-2 text-right text-[12px] text-foreground-dim select-none">
        {lineNumber(row.new?.no ?? null)}
      </div>
      {#if row.new === null}
        <div class="min-h-[22px] bg-sunk"></div>
      {:else}
        <div class="min-w-0 bg-add-muted px-3 text-add">
          {#if row.new.long_len !== null && !newExpanded}
            <div class="flex min-w-0 items-start gap-2">
              <span class="min-w-0 flex-1 truncate" title={`${splitMarker("new", row.new)}${row.new.text}`}
                >{splitMarker("new", row.new)}{row.new.text}</span
              >
              <Button
                variant="ghost"
                size="xs"
                class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
                onclick={() => onToggleLongRow(newKey)}
              >
                Show full
              </Button>
            </div>
          {:else}
            <div class="flex items-start gap-2">
              <span class="min-w-0 flex-1 break-words whitespace-pre-wrap">
                {splitMarker(
                  "new",
                  row.new,
                )}{#each cellSegments(row.new.text, row.new.spans) as segment (`${newKey}:${segment.text}:${segment.changed}`)}
                  {#if segment.changed}
                    <mark class="rounded-sm bg-add/35 px-0.5 text-add">{segment.text}</mark>
                  {:else}
                    {segment.text}
                  {/if}
                {/each}
              </span>
              {#if row.new.long_len !== null}
                <Button
                  variant="ghost"
                  size="xs"
                  class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
                  onclick={() => onToggleLongRow(newKey)}
                >
                  Collapse
                </Button>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
{/each}
