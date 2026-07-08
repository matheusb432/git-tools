<script lang="ts">
  import type { FileSummary, TabMeta } from "@/shared/api";
  import { filterFiles } from "@/entities/diff-tab";
  import { cn } from "@/shared/lib/utils";
  import { Badge, type BadgeVariant } from "@/shared/ui/badge";
  import { TEST_IDS } from "@/shared/testids";

  type Props = {
    readonly meta: TabMeta;
    readonly filterText: string;
    readonly focusedCommits: ReadonlySet<string>;
    readonly selectedFileIdx: number | null;
    readonly onSelectFile: (fileIdx: number) => void;
    readonly onScrollToFile: (fileIdx: number) => void;
  };

  type FileRow = {
    readonly file: FileSummary;
    readonly fileIdx: number;
  };

  let { meta, filterText, focusedCommits, selectedFileIdx, onSelectFile, onScrollToFile }: Props = $props();

  const rows = $derived<FileRow[]>(
    filterFiles(meta.files, filterText, focusedCommits).map((file) => ({
      file,
      fileIdx: meta.files.indexOf(file),
    })),
  );

  function statusVariant(status: string): BadgeVariant {
    switch (status) {
      case "added":
        return "secondary";
      case "deleted":
        return "destructive";
      default:
        return "outline";
    }
  }

  function handleActivate(fileIdx: number): void {
    onSelectFile(fileIdx);
    onScrollToFile(fileIdx);
  }

  function handleKeydown(event: KeyboardEvent, fileIdx: number): void {
    if (event.key === " ") event.preventDefault();
    if (event.key === "Enter" || event.key === " ") handleActivate(fileIdx);
  }
</script>

<aside
  data-testid={TEST_IDS.diffView.fileTree}
  class="flex min-h-0 flex-col border-r border-border bg-surface/50"
>
  <div class="border-b border-border px-4 py-3">
    <div class="flex items-center justify-between gap-2">
      <h2 class="text-sm font-semibold">Files</h2>
      <Badge variant="outline">{rows.length}</Badge>
    </div>
    <p class="mt-1 text-xs text-foreground-muted">{meta.repo_name || meta.repo_root}</p>
  </div>

  <div class="min-h-0 flex-1 overflow-auto px-2 py-2">
    {#if rows.length > 0}
      <div class="flex flex-col gap-1">
        {#each rows as row (row.file.path)}
          <div
            role="button"
            tabindex="0"
            class={cn(
              "rounded-lg border border-transparent px-3 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent hover:border-border hover:bg-muted/70",
              selectedFileIdx === row.fileIdx && "border-accent/40 bg-muted text-foreground",
            )}
            onclick={() => handleActivate(row.fileIdx)}
            onkeydown={(event) => handleKeydown(event, row.fileIdx)}
          >
            <div class="flex items-start gap-2">
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm font-medium" title={row.file.path}>{row.file.path}</p>
                <div class="mt-1 flex flex-wrap items-center gap-2 text-xs text-foreground-muted">
                  <Badge variant={statusVariant(row.file.status)}>{row.file.status}</Badge>
                  <span class="font-mono text-emerald-300">+{row.file.added}</span>
                  <span class="font-mono text-rose-300">-{row.file.removed}</span>
                  {#if selectedFileIdx === row.fileIdx}
                    <span class="text-accent">Selected</span>
                  {/if}
                </div>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <p class="px-2 py-6 text-sm text-foreground-muted">No files match the current filter.</p>
    {/if}
  </div>
</aside>
