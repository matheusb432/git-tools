<script lang="ts">
  import { untrack } from "svelte";
  import { useSelector } from "@xstate/store-svelte";
  import type { DiffReviewStore } from "../../model/diff-review";
  import type { FileSummary, TabMeta } from "@/shared/api";
  import { filterFiles } from "../../model/file-filter";
  import { cn } from "@/shared/lib/utils";
  import { TEST_IDS } from "@/shared/testids";

  type Props = {
    readonly meta: TabMeta;
    readonly reviewStore: DiffReviewStore;
    readonly onScrollToFile: (fileIdx: number) => void;
  };

  type FileRow = {
    readonly file: FileSummary;
    readonly fileIdx: number;
  };

  let { meta, reviewStore, onScrollToFile }: Props = $props();

  const mountedReviewStore = untrack(() => reviewStore);
  const filterText = useSelector(mountedReviewStore, (snapshot) => snapshot.context.filterText);
  const focusedCommits = useSelector(mountedReviewStore, (snapshot) => snapshot.context.focusedCommits);
  const selectedFileIdx = useSelector(mountedReviewStore, (snapshot) => snapshot.context.selectedFileIdx);

  const rows = $derived<FileRow[]>(
    filterFiles(meta.files, $filterText, $focusedCommits).map((file) => ({
      file,
      fileIdx: meta.files.indexOf(file),
    })),
  );
  const activeFileIdx = $derived(
    $selectedFileIdx !== null && rows.some((row) => row.fileIdx === $selectedFileIdx)
      ? $selectedFileIdx
      : (rows[0]?.fileIdx ?? null),
  );

  function handleActivate(fileIdx: number): void {
    mountedReviewStore.trigger["file.selected"]({ fileIdx });
    onScrollToFile(fileIdx);
  }
</script>

<aside
  data-testid={TEST_IDS.diffView.fileTree}
  class="tree min-h-0 overflow-auto border-r border-border bg-surface p-3 font-mono max-[1024px]:hidden"
>
  <div class="tree-head mt-1 mb-2 flex justify-between text-[11px] tracking-[0.06em] text-foreground-dim uppercase">
    <span>Files</span>
    <span>{rows.length}</span>
  </div>
  <div class="mb-3 flex flex-wrap gap-2 text-[11px]">
    <span class="rounded border border-border-strong px-2 py-0.5 text-foreground-muted"
      ><b class="text-foreground">{rows.length}</b> files</span
    >
    <span class="rounded border border-add/40 px-2 py-0.5 text-add"
      >+{meta.files.reduce((sum, file) => sum + file.added, 0)}</span
    >
    <span class="rounded border border-delete/40 px-2 py-0.5 text-delete"
      >-{meta.files.reduce((sum, file) => sum + file.removed, 0)}</span
    >
  </div>

  {#if rows.length > 0}
    <div class="tree-body flex flex-col">
      {#each rows as row (row.file.path)}
        <button
          type="button"
          class={cn(
            "flex cursor-pointer items-center gap-2 rounded px-2 py-1 text-left text-[12.5px] leading-snug text-foreground-muted hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none",
            activeFileIdx === row.fileIdx && "bg-accent/15 text-foreground shadow-[inset_2px_0_0_var(--color-accent)]",
          )}
          onclick={() => handleActivate(row.fileIdx)}
          title={row.file.path}
        >
          <span class="min-w-0 flex-1 truncate">{row.file.path}</span>
          <span class="rounded border border-border-strong px-1 text-[9.5px] font-bold text-foreground-dim uppercase">
            {row.file.status.slice(0, 1)}
          </span>
          <span class="shrink-0 text-add">+{row.file.added}</span>
          <span class="shrink-0 text-delete">-{row.file.removed}</span>
        </button>
      {/each}
    </div>
  {:else}
    <p class="px-2 py-6 text-[12.5px] text-foreground-muted">No files match the current filter.</p>
  {/if}
</aside>
