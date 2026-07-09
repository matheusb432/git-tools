<script lang="ts">
  import { onMount, tick } from "svelte";
  import { AlertTriangle, RefreshCw } from "@lucide/svelte";
  import { emptyRowPageCache, invalidateTabRows, isLiveTab, type RowPageCache } from "@/entities/diff-tab";
  import type { BrokenSourceCode, NativeTab } from "@/entities/diff-tab";
  import { filterFiles } from "@/entities/diff-tab";
  import { fileRows, getSetting, setSetting, type RowsPage, type SplitRow, type UnifiedRow } from "@/shared/api";
  import * as Alert from "@/shared/ui/alert";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import { Skeleton } from "@/shared/ui/skeleton";
  import { TEST_IDS } from "@/shared/testids";
  import {
    clearCommitFocus,
    activePanelFileIndexes,
    allFilePanelsCollapsed,
    copyPageAccumulatorFromCache,
    copySplitRows,
    copyUnifiedRows,
    createDiffViewState,
    expandFilePanel,
    filePanelIsExpanded,
    fullFromSetting,
    layoutFromSetting,
    missingCopyPageRequestsForAccumulator,
    nextFileIndex,
    orderedCopyPagesFromAccumulator,
    putCopyPageInAccumulator,
    setFilterText,
    setLayout,
    settingFromFull,
    settingFromLayout,
    toggleAllFilePanels,
    toggleCommitFocus,
    toggleFull,
    type DiffViewState,
    type FilePanelFoldState,
    type DiffLayout,
  } from "../model/diff-view";
  import CommitShelf from "./CommitShelf.svelte";
  import DiffToolbar from "./DiffToolbar.svelte";
  import FileDiffPanel from "./FileDiffPanel.svelte";
  import FileTree from "./FileTree.svelte";

  type DiffViewProps = {
    readonly tab: NativeTab;
    readonly onRefresh: (tab: NativeTab) => Promise<void>;
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

  let { tab, onRefresh }: DiffViewProps = $props();

  let viewState = $state<DiffViewState>(createDiffViewState());
  let selectedFileIdx = $state<number | null>(null);
  let contentRef = $state<HTMLElement | null>(null);
  let rowPageCache = $state.raw<RowPageCache>(emptyRowPageCache());
  let foldState = $state.raw<FilePanelFoldState>({ collapsedFileIdxs: new Set<number>() });
  let cacheBatchId = $state<string | null>(null);
  let cacheTabId = $state<number | null>(null);
  let settingsWarning = $state<string | null>(null);

  const COPY_PAGE_SIZE = 80;

  const meta = $derived(tab.meta);
  const brokenSource = $derived(isLiveTab(tab) ? (tab.brokenSource ?? null) : null);
  const visibleFiles = $derived<VisibleFile[]>(
    meta === null
      ? []
      : filterFiles(meta.files, viewState.filterText, viewState.focusedCommits).map((file) => ({
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
    if (selectedFileIdx !== null && visibleFiles.some((file) => file.fileIdx === selectedFileIdx)) {
      return selectedFileIdx;
    }
    return visibleFiles[0]?.fileIdx ?? null;
  });
  const visibleFileIndexes = $derived(visibleFiles.map((file) => file.fileIdx));
  const activePanelIndexes = $derived(activePanelFileIndexes({ visibleFileIndexes, activeFileIdx }));
  const activePanelFiles = $derived(visibleFiles.filter((file) => activePanelIndexes.includes(file.fileIdx)));
  const allFilesFolded = $derived(allFilePanelsCollapsed(foldState, visibleFileIndexes));

  function persistLayout(layout: DiffLayout): void {
    void setSetting("diff.layout", settingFromLayout(layout))
      .then(() => {
        if (settingsWarning === "Could not save diff view settings.") {
          settingsWarning = null;
        }
      })
      .catch(() => {
        settingsWarning = "Could not save diff view settings.";
      });
  }

  function persistFull(full: boolean): void {
    void setSetting("diff.full", settingFromFull(full))
      .then(() => {
        if (settingsWarning === "Could not save diff view settings.") {
          settingsWarning = null;
        }
      })
      .catch(() => {
        settingsWarning = "Could not save diff view settings.";
      });
  }

  function handleSetLayout(layout: DiffLayout): void {
    viewState = setLayout(viewState, layout);
    persistLayout(layout);
  }

  function handleToggleFull(): void {
    viewState = toggleFull(viewState);
    persistFull(viewState.full);
  }

  function handleFilterTextChange(value: string): void {
    viewState = setFilterText(viewState, value);
  }

  function handleToggleCommitFocus(sha: string): void {
    viewState = toggleCommitFocus(viewState, sha);
  }

  function handleClearCommitFocus(): void {
    viewState = clearCommitFocus(viewState);
  }

  function handleSelectFile(fileIdx: number): void {
    selectedFileIdx = fileIdx;
    foldState = expandFilePanel(foldState, fileIdx);
  }

  function updateRowCache(updater: (cache: RowPageCache) => RowPageCache): void {
    rowPageCache = updater(rowPageCache);
  }

  function sectionId(fileIdx: number): string {
    return `diff-view-${tab.localId}-${fileIdx}`;
  }

  const BROKEN_SOURCE_TITLES: Record<BrokenSourceCode, string> = {
    DirNotFound: "The git repo's directory was not found.",
    DirNotGitRepo: "This directory is not a git repository.",
  };

  function handleScrollToFile(fileIdx: number): void {
    void tick().then(() => {
      const target = contentRef?.querySelector<HTMLElement>(`#${CSS.escape(sectionId(fileIdx))}`);
      target?.scrollIntoView({ block: "start", behavior: "smooth" });
    });
  }

  function copyTextForPages(pages: readonly RowsPage[]): string | null {
    if (viewState.layout === "unified") {
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

  async function fetchCopyPage(args: {
    readonly tabId: number;
    readonly fileIdx: number;
    readonly layout: DiffLayout;
    readonly full: boolean;
    readonly pageStart: number;
    readonly pageSize: number;
  }): Promise<RowsPage> {
    return fileRows({
      tabId: args.tabId,
      fileIdx: args.fileIdx,
      layout: args.layout,
      full: args.full,
      start: args.pageStart,
      count: args.pageSize,
    });
  }

  async function activeFileRowsText(): Promise<string | null> {
    if (tab.tabId === null || activeFileIdx === null) return null;

    const copyTarget = {
      tabId: tab.tabId,
      fileIdx: activeFileIdx,
      layout: viewState.layout,
      full: viewState.full,
    };

    let pages = copyPageAccumulatorFromCache({ rowPageCache, ...copyTarget });
    let firstPage = pages.get(0);

    if (firstPage === undefined) {
      firstPage = await fetchCopyPage({ ...copyTarget, pageStart: 0, pageSize: COPY_PAGE_SIZE });
      pages = putCopyPageInAccumulator(pages, 0, firstPage);
    }

    const missingPages = missingCopyPageRequestsForAccumulator({
      pages,
      pageSize: COPY_PAGE_SIZE,
      total: firstPage.total,
    });

    if (missingPages.length > 0) {
      for (const page of missingPages) {
        const fetched = await fetchCopyPage({ ...copyTarget, pageStart: page.pageStart, pageSize: page.pageSize });
        pages = putCopyPageInAccumulator(pages, page.pageStart, fetched);
      }
    }

    return copyTextForPages(orderedCopyPagesFromAccumulator(pages));
  }

  function selectedVisibleText(): string | null {
    if (contentRef === null) return null;
    const selection = window.getSelection();
    if (selection === null || selection.isCollapsed) return null;
    const text = selection.toString();
    if (text.trim() === "") return null;
    const anchorNode = selection.anchorNode;
    const focusNode = selection.focusNode;
    if (anchorNode === null || focusNode === null) return null;
    if (!contentRef.contains(anchorNode) || !contentRef.contains(focusNode)) return null;
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
    selectedFileIdx = next.fileIdx;
    foldState = expandFilePanel(foldState, next.fileIdx);
    handleScrollToFile(next.fileIdx);
  }

  function handleToggleAllFilesFolded(): void {
    foldState = toggleAllFilePanels(foldState, visibleFileIndexes);
  }

  function handleToggleFilePanel(fileIdx: number): void {
    const collapsedFileIdxs = new Set(foldState.collapsedFileIdxs);
    if (collapsedFileIdxs.has(fileIdx)) collapsedFileIdxs.delete(fileIdx);
    else collapsedFileIdxs.add(fileIdx);
    foldState = { collapsedFileIdxs };
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
        if (viewState.layout !== "unified") handleSetLayout("unified");
        return;
      case "s":
        event.preventDefault();
        if (viewState.layout !== "split") handleSetLayout("split");
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

  $effect(() => {
    const nextBatchId = tab.meta?.batch_id ?? null;
    const nextTabId = tab.tabId;
    if (nextBatchId === cacheBatchId && nextTabId === cacheTabId) return;

    rowPageCache = cacheTabId === null ? emptyRowPageCache() : invalidateTabRows(rowPageCache, cacheTabId);
    cacheBatchId = nextBatchId;
    cacheTabId = nextTabId;
  });

  onMount(() => {
    void Promise.all([getSetting("diff.layout"), getSetting("diff.full")])
      .then(([layout, full]) => {
        viewState = {
          ...viewState,
          layout: layoutFromSetting(layout),
          full: fullFromSetting(full),
        };
        settingsWarning = null;
      })
      .catch(() => {
        settingsWarning = "Could not load diff view settings. Using defaults.";
      });
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div data-testid={TEST_IDS.diffView.root} class="flex h-full min-h-0 flex-col bg-background">
  {#if brokenSource !== null}
    <div class="p-5" data-testid={TEST_IDS.diffView.brokenSource}>
      <Alert.Root variant="destructive" class="max-w-2xl">
        <AlertTriangle />
        <Alert.Title>{BROKEN_SOURCE_TITLES[brokenSource.code]}</Alert.Title>
        <Alert.Description>{brokenSource.reason}</Alert.Description>
      </Alert.Root>
    </div>
  {:else if tab.lifecycle.state === "error" && meta === null}
    <div class="p-5">
      <Alert.Root variant="destructive" class="max-w-2xl">
        <AlertTriangle />
        <Alert.Title>Native diff failed to load</Alert.Title>
        <Alert.Description>{tab.lifecycle.message}</Alert.Description>
        <Alert.Action>
          <Button variant="destructive" size="sm" onclick={() => onRefresh(tab)}>
            <RefreshCw data-icon="inline-start" />
            Retry
          </Button>
        </Alert.Action>
      </Alert.Root>
    </div>
  {:else if tab.lifecycle.state === "opening" || meta === null}
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
    {#if settingsWarning !== null}
      <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-5 py-2 text-xs text-foreground-muted">
        <AlertTriangle class="size-3.5" />
        <span>{settingsWarning}</span>
      </div>
    {/if}

    <DiffToolbar
      {meta}
      layout={viewState.layout}
      full={viewState.full}
      filterText={viewState.filterText}
      refreshing={tab.lifecycle.state === "refreshing"}
      canCopy={activeFileIdx !== null}
      {allFilesFolded}
      onSetLayout={handleSetLayout}
      onToggleFull={handleToggleFull}
      onToggleAllFilesFolded={handleToggleAllFilesFolded}
      onFilterTextChange={handleFilterTextChange}
      onRefresh={() => onRefresh(tab)}
      onCopyVisible={copyCurrentRows}
    />

    {#if tab.lifecycle.state === "error"}
      <div class="px-5 pt-4">
        <Alert.Root variant="destructive">
          <AlertTriangle />
          <Alert.Title>Refresh failed</Alert.Title>
          <Alert.Description>{tab.lifecycle.message}</Alert.Description>
          <Alert.Action>
            <Button variant="destructive" size="sm" onclick={() => onRefresh(tab)}>
              <RefreshCw data-icon="inline-start" />
              Retry
            </Button>
          </Alert.Action>
        </Alert.Root>
      </div>
    {/if}

    <CommitShelf
      commits={meta.commits}
      focusedCommits={viewState.focusedCommits}
      onToggleCommitFocus={handleToggleCommitFocus}
      onClearFocus={handleClearCommitFocus}
    />

    <div class="grid min-h-0 flex-1 lg:grid-cols-[300px_minmax(0,1fr)]">
      <FileTree
        {meta}
        filterText={viewState.filterText}
        focusedCommits={viewState.focusedCommits}
        selectedFileIdx={activeFileIdx}
        onSelectFile={handleSelectFile}
        onScrollToFile={handleScrollToFile}
      />

      <section data-testid={TEST_IDS.diffView.content} bind:this={contentRef} class="min-h-0 overflow-auto px-5 py-4">
        <div class="mb-4 flex items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-semibold">Files in view</h2>
            <p class="mt-1 text-xs text-foreground-muted">
              {meta.foot_note || "Select a file to inspect its paged diff rows."}
            </p>
          </div>
          <div class="flex items-center gap-2">
            <Badge variant="outline">{visibleFiles.length} files</Badge>
            <Badge variant={viewState.layout === "split" ? "secondary" : "outline"}>{viewState.layout}</Badge>
            <Badge variant={viewState.full ? "secondary" : "outline"}>{viewState.full ? "full" : "compact"}</Badge>
          </div>
        </div>

        {#if activePanelFiles.length > 0}
          <div class="flex flex-col gap-3">
            {#each activePanelFiles as file (file.fileIdx)}
              <section id={sectionId(file.fileIdx)} class="scroll-mt-4">
                {#if tab.tabId !== null}
                  <FileDiffPanel
                    tabId={tab.tabId}
                    fileIdx={file.fileIdx}
                    path={file.path}
                    status={file.status}
                    added={file.added}
                    removed={file.removed}
                    hasFull={file.hasFull}
                    commits={file.commits}
                    layout={viewState.layout}
                    full={viewState.full}
                    selected={activeFileIdx === file.fileIdx}
                    expanded={filePanelIsExpanded(foldState, file.fileIdx)}
                    onToggleExpanded={() => handleToggleFilePanel(file.fileIdx)}
                    rowCache={rowPageCache}
                    {updateRowCache}
                  />
                {/if}
              </section>
            {/each}
          </div>
        {:else}
          <div class="rounded-lg border border-dashed border-border px-4 py-8 text-sm text-foreground-muted">
            No files match the current filter and commit focus.
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>
