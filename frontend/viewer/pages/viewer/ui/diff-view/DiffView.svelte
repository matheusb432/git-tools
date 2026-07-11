<script lang="ts">
  import { useSelector } from "@xstate/store-svelte";
  import { useQueryClient } from "@tanstack/svelte-query";
  import { AlertTriangle, RefreshCw } from "@lucide/svelte";
  import { untrack } from "svelte";
  import { createSetViewerSetting, createViewerSettings, viewerSettingsFromQuery } from "../../api/settings.svelte";
  import { rowPageOptions } from "../../api/row-pages.svelte";
  import { copySplitRows, copyUnifiedRows } from "../../model/copy-rows";
  import type { DiffReviewStore } from "../../model/diff-review";
  import { filterFiles } from "../../model/file-filter";
  import { nextFileIndex, type DiffLayout } from "../../model/file-panels";
  import type { BrokenSourceCode } from "../../model/live-tab";
  import type { TabShell } from "../../model/viewer-session";
  import { useApi, type RowsPage, type SplitRow, type TabMeta, type UnifiedRow } from "@/shared/api";
  import * as Alert from "@/shared/ui/alert";
  import { Button } from "@/shared/ui/button";
  import { Skeleton } from "@/shared/ui/skeleton";
  import { TEST_IDS } from "@/shared/testids";
  import CommitShelf from "./CommitShelf.svelte";
  import DiffKeybar from "./DiffKeybar.svelte";
  import DiffToolbar from "./DiffToolbar.svelte";
  import FileTree from "./FileTree.svelte";
  import VirtualFileList from "./VirtualFileList.svelte";
  import type { VirtualFileListHandle } from "./VirtualFileList.svelte";

  type DiffTab = TabShell & {
    readonly meta: TabMeta | null;
    readonly isRefreshing: boolean;
  };

  type DiffViewProps = {
    readonly tab: DiffTab;
    readonly reviewStore: DiffReviewStore;
    readonly onRefresh: (tab: DiffTab) => Promise<void>;
  };

  type VisibleFile = {
    readonly fileIdx: number;
    readonly path: string;
    readonly status: string;
    readonly added: number;
    readonly removed: number;
    readonly hasFull: boolean;
    readonly commits: readonly string[];
  };

  let { tab, reviewStore, onRefresh }: DiffViewProps = $props();

  const mountedReviewStore = untrack(() => reviewStore);
  const filterText = useSelector(mountedReviewStore, (snapshot) => snapshot.context.filterText);
  const selectedFileIdx = useSelector(mountedReviewStore, (snapshot) => snapshot.context.selectedFileIdx);
  const focusedCommits = useSelector(mountedReviewStore, (snapshot) => snapshot.context.focusedCommits);
  const collapsedFileIdxs = useSelector(mountedReviewStore, (snapshot) => snapshot.context.collapsedFileIdxs);
  const settingsQuery = createViewerSettings();
  const settingMutation = createSetViewerSetting();
  const api = useApi();
  const queryClient = useQueryClient();

  let contentRef = $state<HTMLElement | null>(null);
  let fileListHandle = $state<VirtualFileListHandle | null>(null);

  const COPY_PAGE_SIZE = 80;

  const meta = $derived(tab.meta);
  const settings = $derived(viewerSettingsFromQuery($settingsQuery.isError, $settingsQuery.data));
  const layout = $derived(settings.layout);
  const density = $derived(settings.density);
  const full = $derived(density === "full");
  const settingsWarning = $derived(
    $settingsQuery.isError
      ? "Could not load diff view settings. Using defaults."
      : $settingMutation.isError
        ? "Could not save diff view settings."
        : null,
  );
  const brokenSource = $derived(tab.live?.brokenSource ?? null);
  const visibleFiles = $derived<VisibleFile[]>(
    meta === null
      ? []
      : filterFiles(meta.files, $filterText, $focusedCommits).map((file) => ({
          fileIdx: meta.files.indexOf(file),
          path: file.path,
          status: file.status,
          added: file.added,
          removed: file.removed,
          hasFull: file.has_full,
          commits: file.commits,
        })),
  );

  const activeFileIdx = $derived.by(() => {
    if (visibleFiles.length === 0) return null;
    if ($selectedFileIdx !== null && visibleFiles.some((file) => file.fileIdx === $selectedFileIdx)) {
      return $selectedFileIdx;
    }
    return visibleFiles[0]?.fileIdx ?? null;
  });
  const visibleFileIndexes = $derived(visibleFiles.map((file) => file.fileIdx));
  const allFilesFolded = $derived(
    visibleFileIndexes.length > 0 && visibleFileIndexes.every((fileIdx) => $collapsedFileIdxs.has(fileIdx)),
  );

  function handleSetLayout(nextLayout: DiffLayout): void {
    $settingMutation.mutate({ key: "layout", value: nextLayout });
  }

  function handleToggleFull(): void {
    $settingMutation.mutate({ key: "density", value: density === "full" ? "compact" : "full" });
  }

  function handleFilterTextChange(value: string): void {
    reviewStore.trigger["filter.changed"]({ value });
  }

  const BROKEN_SOURCE_TITLES: Record<BrokenSourceCode, string> = {
    DirNotFound: "The git repo's directory was not found.",
    DirNotGitRepo: "This directory is not a git repository.",
  };

  function handleScrollToFile(fileIdx: number): void {
    fileListHandle?.scrollToFile(fileIdx);
  }

  function copyTextForPages(pages: readonly RowsPage[]): string | null {
    if (layout === "unified") {
      const rows: UnifiedRow[] = [];
      for (const page of pages) {
        if (page.layout === "unified") rows.push(...page.rows);
      }
      return rows.length > 0 ? copyUnifiedRows(rows) : null;
    }

    const rows: SplitRow[] = [];
    for (const page of pages) {
      if (page.layout === "split") rows.push(...page.rows);
    }
    return rows.length > 0 ? copySplitRows(rows) : null;
  }

  async function activeFileRowsText(): Promise<string | null> {
    if (tab.tabId === null || activeFileIdx === null) return null;

    const copyTarget = {
      tabId: tab.tabId,
      fileIdx: activeFileIdx,
      layout,
      density,
    };
    const firstPage = await queryClient.ensureQueryData(
      rowPageOptions(api, { ...copyTarget, start: 0, count: COPY_PAGE_SIZE }),
    );
    const pages: RowsPage[] = [firstPage];
    for (let start = COPY_PAGE_SIZE; start < firstPage.total; start += COPY_PAGE_SIZE) {
      pages.push(
        await queryClient.ensureQueryData(rowPageOptions(api, { ...copyTarget, start, count: COPY_PAGE_SIZE })),
      );
    }
    return copyTextForPages(pages);
  }

  function selectedVisibleText(): string | null {
    if (contentRef === null) return null;
    const fileList = contentRef.querySelector<HTMLElement>(`[data-testid="${TEST_IDS.diffView.fileList}"]`);
    if (fileList === null) return null;
    const selection = window.getSelection();
    if (selection === null || selection.isCollapsed) return null;
    const text = selection.toString();
    if (text.trim() === "") return null;
    const anchorNode = selection.anchorNode;
    const focusNode = selection.focusNode;
    if (anchorNode === null || focusNode === null) return null;
    if (!fileList.contains(anchorNode) || !fileList.contains(focusNode)) return null;
    return text;
  }

  async function copyCurrentRows(): Promise<void> {
    if (navigator.clipboard?.writeText === undefined) return;
    const visibleText = selectedVisibleText();
    const text = visibleText ?? (await activeFileRowsText());
    if (text === null) return;
    await navigator.clipboard.writeText(text);
  }

  function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
    return target.isContentEditable || target.closest("[contenteditable='true']") !== null;
  }

  function moveSelection(direction: "previous" | "next"): void {
    if (visibleFiles.length === 0) return;
    const current =
      activeFileIdx === null
        ? 0
        : Math.max(
            0,
            visibleFiles.findIndex((file) => file.fileIdx === activeFileIdx),
          );
    const next = visibleFiles[nextFileIndex({ current, direction, total: visibleFiles.length })];
    if (next === undefined) return;
    reviewStore.trigger["file.selected"]({ fileIdx: next.fileIdx });
    handleScrollToFile(next.fileIdx);
  }

  function handleToggleAllFilesFolded(): void {
    reviewStore.trigger["panels.toggledAll"]({ visibleFileIdxs: visibleFileIndexes });
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey || isEditableTarget(event.target))
      return;

    switch (event.key) {
      case "j":
      case "ArrowDown":
        event.preventDefault();
        moveSelection("next");
        return;
      case "k":
      case "ArrowUp":
        event.preventDefault();
        moveSelection("previous");
        return;
      case "u":
        event.preventDefault();
        if (layout !== "unified") handleSetLayout("unified");
        return;
      case "s":
        event.preventDefault();
        if (layout !== "split") handleSetLayout("split");
        return;
      case "f":
        event.preventDefault();
        handleToggleFull();
        return;
      case "c":
        event.preventDefault();
        void copyCurrentRows();
        return;
      default:
        return;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div bind:this={contentRef} class="flex h-full min-h-0 flex-col bg-background">
  {#if brokenSource !== null}
    <div class="p-5" data-testid={TEST_IDS.diffView.brokenSource}>
      <Alert.Root variant="destructive" class="max-w-2xl">
        <AlertTriangle />
        <Alert.Title>{BROKEN_SOURCE_TITLES[brokenSource.code]}</Alert.Title>
        <Alert.Description>{brokenSource.reason}</Alert.Description>
      </Alert.Root>
    </div>
  {:else if tab.failure !== null && meta === null}
    <div class="p-5">
      <Alert.Root variant="destructive" class="max-w-2xl">
        <AlertTriangle />
        <Alert.Title>Native diff failed to load</Alert.Title>
        <Alert.Description>{tab.failure}</Alert.Description>
        <Alert.Action>
          <Button variant="destructive" size="sm" onclick={() => onRefresh(tab)}>
            <RefreshCw data-icon="inline-start" />
            Retry
          </Button>
        </Alert.Action>
      </Alert.Root>
    </div>
  {:else if tab.tabId === null || meta === null || $settingsQuery.isPending}
    <div class="grid min-h-0 flex-1 gap-4 p-5 lg:grid-cols-[260px_minmax(0,1fr)]">
      <div class="flex flex-col gap-3">
        <Skeleton class="h-10 w-full rounded-lg" />
        <Skeleton class="h-28 w-full rounded-lg" />
        <Skeleton class="h-28 w-full rounded-lg" />
      </div>
      <div class="flex flex-col gap-4">
        <Skeleton class="h-20 w-full rounded-lg" />
        <Skeleton class="h-[420px] w-full rounded-lg" />
      </div>
    </div>
  {:else}
    <div
      data-testid={TEST_IDS.diffView.root}
      class="grid h-full min-h-0 grid-cols-[262px_minmax(0,1fr)_252px] grid-rows-[auto_1fr_auto] bg-background text-foreground max-[1024px]:grid-cols-[minmax(0,1fr)] max-[1024px]:grid-rows-[auto_minmax(0,1fr)_auto]"
    >
      <div class="col-span-3 max-[1024px]:col-span-1">
        <DiffToolbar
          {meta}
          {layout}
          {full}
          filterText={$filterText}
          refreshing={tab.isRefreshing}
          canCopy={activeFileIdx !== null}
          {allFilesFolded}
          onSetLayout={handleSetLayout}
          onToggleFull={handleToggleFull}
          onToggleAllFilesFolded={handleToggleAllFilesFolded}
          onFilterTextChange={handleFilterTextChange}
          onRefresh={() => onRefresh(tab)}
          onCopyVisible={copyCurrentRows}
        />

        {#if settingsWarning !== null}
          <div
            class="flex items-center gap-2 border-b border-border bg-muted/40 px-5 py-2 text-xs text-foreground-muted"
          >
            <AlertTriangle class="size-3.5" />
            <span>{settingsWarning}</span>
          </div>
        {/if}

        {#if tab.failure !== null}
          <div class="border-b border-border px-5 py-4">
            <Alert.Root variant="destructive">
              <AlertTriangle />
              <Alert.Title>Refresh failed</Alert.Title>
              <Alert.Description>{tab.failure}</Alert.Description>
              <Alert.Action>
                <Button variant="destructive" size="sm" onclick={() => onRefresh(tab)}>
                  <RefreshCw data-icon="inline-start" />
                  Retry
                </Button>
              </Alert.Action>
            </Alert.Root>
          </div>
        {/if}
      </div>

      <div class="contents max-[1024px]:hidden">
        <FileTree {meta} {reviewStore} onScrollToFile={handleScrollToFile} />
      </div>

      {#if tab.tabId !== null}
        <VirtualFileList
          bind:handle={fileListHandle}
          tabId={tab.tabId}
          files={visibleFiles}
          {reviewStore}
          {layout}
          {full}
        />
      {/if}

      <div class="contents max-[1024px]:hidden">
        <CommitShelf commits={meta.commits} {reviewStore} />
      </div>

      <div class="col-span-3 max-[1024px]:col-span-1">
        <DiffKeybar {meta} />
      </div>
    </div>
  {/if}
</div>
