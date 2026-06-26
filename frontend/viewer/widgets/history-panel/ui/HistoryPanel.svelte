<script lang="ts">
  import { Search, ChevronDown, Clock } from "@lucide/svelte";
  import { cn } from "@/shared/lib/utils";
  import { groupHistoryByRepo, historyTabLabel, type HistoryEntry } from "@/entities/diff";
  import { relativeTime } from "@/shared/lib/time";
  import { formatBytes } from "@/shared/lib/format";
  import { filterHistory } from "../model/filter";

  type Props = { history: HistoryEntry[]; now: Date; onOpen: (url: string, label: string) => void };
  let { history, now, onOpen }: Props = $props();

  let query = $state("");
  let collapsed = $state(new Set<string>());

  const groups = $derived([...groupHistoryByRepo(filterHistory(history, query)).entries()]);

  // Use the content-hash segment (last path segment) for the unnamed-diff label so
  // every entry in a repo gets a distinct 12-char prefix rather than reusing the repo_id.
  const fallback = (url: string) => url.split("/").pop()?.slice(0, 12) ?? url;

  // "3-dot" → sky, "2-dot" → violet, "worktree" → muted (default).
  // There is no "merge" kind; DiffKind is exactly TwoDot | ThreeDot | WorkTree.
  const badgeClass = (kind: string) =>
    kind === "3-dot" ? "text-sky-300 border-sky-300/35"
    : kind === "2-dot" ? "text-violet-300 border-violet-300/35"
    : "text-foreground-muted border-border-strong";

  function toggle(repo: string) {
    const next = new Set(collapsed);
    next.has(repo) ? next.delete(repo) : next.add(repo);
    collapsed = next;
  }
</script>

<div class="overflow-auto px-6 py-5 h-full">
  <div class="flex items-center gap-2.5 w-full px-3.5 py-2.5 mb-6 rounded-[10px] bg-surface border border-border text-foreground-muted focus-within:border-accent">
    <Search class="size-4" />
    <input
      class="w-full bg-transparent text-sm outline-none placeholder:text-foreground-muted"
      placeholder="Search diffs by name, repo, or range…"
      bind:value={query}
    />
  </div>

  {#each groups as [repo, rows] (repo)}
    <div class="mb-5">
      <button
        type="button"
        class="flex items-center gap-2.5 mb-2.5 w-full rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
        onclick={() => toggle(repo)}
      >
        <ChevronDown
          class={cn(
            "size-3.5 text-foreground-muted motion-safe:transition-transform",
            collapsed.has(repo) && "-rotate-90",
          )}
        />
        <span class="text-xs font-bold tracking-wider uppercase text-accent">{repo}</span>
        <span class="text-[11px] font-semibold text-foreground-muted bg-muted px-2 py-0.5 rounded-full">
          {rows.length} diffs
        </span>
      </button>

      {#if !collapsed.has(repo)}
        {#each rows as row (row.url)}
          <div
            role="button"
            tabindex="0"
            class="flex items-center gap-3.5 px-3.5 py-3 rounded-[9px] border border-transparent cursor-pointer hover:bg-muted hover:border-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            onclick={() => onOpen(row.url, historyTabLabel(row, fallback))}
            onkeydown={(e) => {
              if (e.key === " ") e.preventDefault();
              if (e.key === "Enter" || e.key === " ") onOpen(row.url, historyTabLabel(row, fallback));
            }}
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
            <span class="ml-auto flex items-center gap-3 shrink-0">
              <span class={cn("text-[11px] font-semibold px-2.5 py-0.5 rounded-full border", badgeClass(row.kind))}>
                {row.kind}
              </span>
              <span class="font-mono text-xs text-foreground-muted">{row.range_label}</span>
              <span class="font-mono text-xs text-foreground-muted w-12 text-right">{formatBytes(row.byte_size)}</span>
              <span
                class="flex items-center gap-1.5 text-[12.5px] text-foreground-muted w-24"
                title={row.head_committed_at}
              >
                <Clock class="size-3.5" />{relativeTime(row.head_committed_at, now)}
              </span>
            </span>
          </div>
        {/each}
      {/if}
    </div>
  {/each}

  {#if groups.length === 0}
    <p class="text-center text-foreground-muted text-sm mt-12">{query ? `No diffs match "${query}".` : "No history yet."}</p>
  {/if}
</div>
