<script lang="ts">
  import { Search, Clock } from "@lucide/svelte";
  import { cn } from "@/shared/lib/utils";
  import { historyTabLabel, type HistoryEntry } from "@/entities/diff";
  import { relativeTime } from "@/shared/lib/time";
  import { formatBytes } from "@/shared/lib/format";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import * as InputGroup from "@/shared/ui/input-group";
  import { TEST_IDS } from "@/shared/testids";
  import { filterHistory } from "../model/filter";

  type Props = { history: HistoryEntry[]; now: Date; onOpen: (url: string, label: string) => void };
  let { history, now, onOpen }: Props = $props();

  let query = $state("");

  // Flat, most-recent-first — `history` already arrives sorted that way (the
  // backend's contract); no per-repo grouping, so `diff subrepos`/`diff all`
  // rows sit wherever their recency puts them instead of always trailing.
  const rows = $derived(filterHistory(history, query));

  // Use the content-hash segment (last path segment) for the unnamed-diff label so
  // every entry in a repo gets a distinct 12-char prefix rather than reusing the repo_id.
  const fallback = (url: string) => url.split("/").pop()?.slice(0, 12) ?? url;

  // "3-dot" → sky, "2-dot" → violet, "worktree" → muted (default).
  // There is no "merge" kind; DiffKind is exactly TwoDot | ThreeDot | WorkTree.
  const badgeClass = (kind: string) =>
    kind === "3-dot" ? "text-sky-300 border-sky-300/35"
    : kind === "2-dot" ? "text-violet-300 border-violet-300/35"
    : "text-foreground-muted border-border-strong";

  const gridCols = "grid-cols-[minmax(0,1fr)_140px_84px_150px_72px_108px]";

  // `diff subrepos`/`diff all` span multiple repos, so they carry no single head
  // commit — `head_committed_at` is empty. Fall back to `generated_at`, mirroring
  // the backend's sort so the displayed recency matches the row's position.
  const recencyOf = (row: HistoryEntry) => row.head_committed_at || row.generated_at;
</script>

<div data-testid={TEST_IDS.historyPanel.root} class="h-full overflow-auto px-6 py-5">
  <InputGroup.Root class="mb-4 h-10 max-w-xl">
    <InputGroup.Input
      placeholder="Search diffs by name, repo, or range..."
      bind:value={query}
      aria-label="Search diff history"
      data-testid={TEST_IDS.historyPanel.search}
    />
    <InputGroup.Addon>
      <Search class="text-foreground-muted" />
    </InputGroup.Addon>
  </InputGroup.Root>

  {#if rows.length > 0}
    <div class={cn("grid gap-3 px-3.5 py-2 mb-1 text-[11px] font-semibold uppercase tracking-wider text-foreground-muted border-b border-border", gridCols)}>
      <span>Diff</span>
      <span>Repo</span>
      <span>Kind</span>
      <span>Range</span>
      <span class="text-right">Size</span>
      <span>Updated</span>
    </div>

    {#each rows as row (row.url)}
      <Button
        data-testid={TEST_IDS.historyPanel.row}
        variant="ghost"
        class={cn("grid gap-3 items-center px-3.5 py-2.5 rounded-[9px] border border-transparent cursor-pointer hover:bg-muted hover:border-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent", gridCols)}
        onclick={() => onOpen(row.url, historyTabLabel(row, fallback))}
      >
        <span
          class={cn(
            "text-sm font-medium min-w-0 truncate",
            (row.title.trim() === "" || row.title === "diff" || row.title === "merge-diff") &&
              "text-foreground-muted font-mono font-normal",
          )}
        >
          {historyTabLabel(row, fallback)}
        </span>
        <span class="text-xs text-foreground-muted truncate" title={row.repo_name || row.repo_id}>
          {row.repo_name || row.repo_id}
        </span>
        <Badge variant="outline" class={cn("w-fit text-[11px]", badgeClass(row.kind))}>
          {row.kind}
        </Badge>
        <span class="font-mono text-xs text-foreground-muted truncate">{row.range_label}</span>
        <span class="font-mono text-xs text-foreground-muted text-right">{formatBytes(row.byte_size)}</span>
        <span class="flex items-center gap-1.5 text-[12.5px] text-foreground-muted" title={recencyOf(row)}>
          <Clock class="size-3.5" />{relativeTime(recencyOf(row), now)}
        </span>
      </Button>
    {/each}
  {:else}
    <p class="text-center text-foreground-muted text-sm mt-12">{query ? `No diffs match "${query}".` : "No history yet."}</p>
  {/if}
</div>
