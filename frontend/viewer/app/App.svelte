<script lang="ts">
  import { flushSync, onMount, tick } from "svelte";
  import { DiffView } from "@/widgets/diff-view";
  import { TabStrip } from "@/widgets/tab-strip";
  import { batchMessage, createToast, dismissFrom, Toast, type ToastItem } from "@/shared/ui/toast";
  import {
    closeNativeTab,
    drainPendingRecipes,
    listenOpenRecipe,
    listLiveViews,
    openRecipe,
    probeSource,
    refreshTab,
    type OpenRecipes,
    type Recipe,
  } from "@/shared/api";
  import {
    beginNativeTabRetry,
    applyOpenedNativeTab,
    brokenSourceFromRejection,
    clearTabFreshness,
    closeAllViewerTabs,
    closeOtherViewerTabs,
    closeViewerTabState,
    dedupeLiveTabBySource,
    isLiveTab,
    liveTabBroken,
    liveTabFromDto,
    markBatchFresh,
    nativeRefreshSucceeded,
    nativeOpeningTab,
    nativeTabError,
    sortViewerTabsByTime,
    viewerTabKey,
    type LiveTab,
    type NativeTab,
    type ViewerTab,
    type ViewerTabState,
  } from "@/entities/diff-tab";

  type ViewerTestApi = {
    openNativeRecipe: (recipe: Recipe, batchId?: string) => Promise<void>;
    activateNativeRepo: (repoName: string) => Promise<boolean>;
    announceBatch: (count: number, commandLabel: string) => void;
    /** Drives the real `--all`/`subrepos` batch-open path (freshness marking + toast) end to
     * end — the same function `listenOpenRecipe`/`drainPendingRecipes` calls from a real CLI
     * batch. Test-only surface: there is no Tauri event-emission hook in this harness. */
    openRecipeBatch: (batch: OpenRecipes) => Promise<void>;
    /** Focuses a restored live tab by its saved source path — the only handle a not-yet-computed
     * live tab exposes, since its title/repo name aren't known until it computes. Calls the same
     * `focusTab` a real tab click does. Test-only: a real user clicks the tab strip instead. */
    activateLiveTabBySource: (sourceValue: string) => boolean;
    snapshot: () => {
      readonly active: number;
      readonly activeTab: {
        readonly kind: ViewerTab["kind"];
        readonly tabId?: number | null;
        readonly lifecycle?: NativeTab["lifecycle"]["state"];
        readonly repoName?: string | null;
      } | null;
      readonly shellError: string | null;
      readonly browserErrors: readonly string[];
      readonly tabs: readonly {
        readonly kind: ViewerTab["kind"];
        readonly localId?: string;
        readonly tabId?: number | null;
        readonly batchId?: string;
        readonly lifecycle?: NativeTab["lifecycle"]["state"];
        readonly error?: string;
        readonly repoName?: string | null;
        readonly fileCount?: number | null;
        readonly live?: boolean;
        readonly source?: { readonly kind: string; readonly value: string } | null;
      }[];
      readonly toasts: readonly { readonly id: number; readonly message: string }[];
    };
  };

  let tabs = $state<ViewerTab[]>([]);
  let active = $state(0);
  let shellError = $state<string | null>(null);
  let unlistenRecipe: (() => void) | undefined;

  const browserErrors: string[] = [];

  let toasts = $state<ToastItem[]>([]);
  let nextToastId = 0;

  function dismissToast(id: number): void {
    toasts = dismissFrom(toasts, id);
  }

  // Announces a batch-open only when more than one diff was opened at once, auto-dismissing
  // after the toast's own timeout.
  function announceBatch(count: number, commandLabel: string): void {
    const message = batchMessage(count, commandLabel);
    if (message === null) return;
    const toast = createToast(nextToastId++, message);
    toasts = [...toasts, toast];
    setTimeout(() => dismissToast(toast.id), toast.timeoutMs);
  }

  const nativeViewKey = (tab: Extract<ViewerTab, { kind: "native" }>) =>
    `${tab.localId}:${tab.batchId}:${tab.tabId ?? "opening"}`;

  // Reactive derived for the currently visible tab; narrows to Tab | undefined so
  // the template can narrow it cleanly without array-index unsafety.
  const activeTab = $derived<ViewerTab | undefined>(tabs[active]);

  // Re-sorts tabs newest-first, then makes `focusKey`'s tab active — so a background
  // relabel/retime can reorder the strip without stealing the user's focus.
  function applyTabs(next: readonly ViewerTab[], focusKey: string): void {
    tabs = [...sortViewerTabsByTime(next)];
    const i = tabs.findIndex((tab) => viewerTabKey(tab) === focusKey);
    active = i >= 0 ? i : Math.min(active, Math.max(tabs.length - 1, 0));
  }

  function apply(next: ViewerTabState): void {
    tabs = [...next.tabs];
    active = next.active;
  }

  function replaceNativeTab(localId: string, update: (tab: Extract<ViewerTab, { kind: "native" }>) => ViewerTab): void {
    const focusKey = `native:${localId}`;
    let changed = false;
    const next = tabs.map((tab) => {
      if (tab.kind !== "native" || tab.localId !== localId) return tab;
      changed = true;
      return update(tab);
    });
    if (changed) applyTabs(next, focusKey);
  }

  async function resolveOpenedNativeTab(
    localId: string,
    opened: Awaited<ReturnType<typeof openRecipe>>,
  ): Promise<void> {
    const { next, openedTabIdToClose } = applyOpenedNativeTab({ tabs, active }, localId, opened);
    apply(next);
    if (openedTabIdToClose !== null) {
      try {
        await closeNativeTab(openedTabIdToClose);
      } catch {
        shellError = "Failed to close one or more native tabs";
      }
    }
  }

  async function closeNativeTabs(tabsToClose: readonly ViewerTab[]): Promise<void> {
    const nativeIds = tabsToClose.flatMap((tab) => (tab.kind === "native" && tab.tabId !== null ? [tab.tabId] : []));

    if (nativeIds.length === 0) return;

    const results = await Promise.allSettled(nativeIds.map((tabId) => closeNativeTab(tabId)));
    if (results.some((result) => result.status === "rejected")) {
      shellError = "Failed to close one or more native tabs";
    }
  }

  async function handleClose(index: number): Promise<void> {
    const tab = tabs[index];
    if (tab === undefined) return;
    apply(closeViewerTabState({ tabs, active }, index));
    await closeNativeTabs([tab]);
  }

  async function handleCloseOthers(index: number): Promise<void> {
    const keep = tabs[index];
    if (keep === undefined) return;
    const toClose = tabs.filter((_, tabIndex) => tabIndex !== index);
    apply(closeOtherViewerTabs({ tabs, active }, index));
    await closeNativeTabs(toClose);
  }

  async function handleCloseAll(): Promise<void> {
    const toClose = [...tabs];
    apply(closeAllViewerTabs({ tabs, active }));
    await closeNativeTabs(toClose);
  }

  async function openNativeRecipe(recipe: Recipe, batchId: string = crypto.randomUUID()): Promise<void> {
    const localId = crypto.randomUUID();
    const opening = nativeOpeningTab(localId, recipe, batchId);
    applyTabs([...tabs, opening], viewerTabKey(opening));
    shellError = null;
    flushSync();
    await tick();

    try {
      const opened = await openRecipe(recipe, batchId);
      await resolveOpenedNativeTab(localId, opened);
    } catch (e) {
      const message = e instanceof Error ? e.message : "Failed to open diff";
      replaceNativeTab(localId, (tab) => nativeTabError(tab, message));
    }

    flushSync();
    await tick();
  }

  // Best-effort short command label for a batch's toast — a single recipe names its own op
  // (e.g. "gtl diff"); a mixed or multi-repo batch falls back to a repo count.
  function opCommandLabel(op: Recipe["op"]): string {
    switch (op.op) {
      case "diff":
        return "gtl diff";
      case "merge-diff":
        return "gtl merge-diff";
      case "squash-preview":
        return "gtl squash-preview";
    }
  }

  function batchLabel(recipes: readonly Recipe[]): string {
    const first = recipes[0];
    if (first === undefined) return "gtl diff";
    if (recipes.length === 1) return opCommandLabel(first.op);
    const sameOp = recipes.every((recipe) => recipe.op.op === first.op.op);
    return `${sameOp ? opCommandLabel(first.op) : "gtl diff"} (${recipes.length} repos)`;
  }

  // Opens every recipe in a CLI-forwarded batch under its shared `batchId`, marks the whole
  // batch as freshly-opened (the tab-strip "new" cue), then announces it — a no-op toast for
  // a single-recipe batch, a "gtl diff (N repos)"-style toast for a multi-repo one.
  async function openRecipeBatch({ batchId, recipes }: OpenRecipes): Promise<void> {
    for (const recipe of recipes) {
      await openNativeRecipe(recipe, batchId);
    }
    tabs = markBatchFresh(tabs, batchId);
    announceBatch(recipes.length, batchLabel(recipes));
  }

  // Restored live tabs are pushed at the `opening` lifecycle with no compute in flight —
  // this set tracks which restored `localId`s still owe a first compute, so
  // `triggerLiveActivation` fires it exactly once, on first focus, never on push.
  const pendingLiveActivation = new Set<string>();

  // Computes a live tab's recipe — but probes the source's directory first, so a live view
  // restored over a moved/deleted repo surfaces the typed broken-source alert instead of a
  // generic "Failed to open diff". A compute (or probe) failure lands on the tab's own
  // `nativeTabError`/`brokenSource` state rather than the shell-wide `shellError` — a broken
  // live source is per-tab, not fatal to the app.
  async function computeLiveTab(tab: LiveTab): Promise<void> {
    try {
      const probe = await probeSource(tab.source.kind, tab.source.value);
      if (probe.outcome === "broken") {
        const brokenSource = brokenSourceFromRejection(probe.code, probe.reason) ?? {
          code: "DirNotFound" as const,
          reason: probe.reason,
        };
        replaceNativeTab(tab.localId, (current) =>
          isLiveTab(current) ? liveTabBroken(current, brokenSource) : nativeTabError(current, probe.reason),
        );
        return;
      }

      const opened = await openRecipe(tab.recipe, tab.batchId);
      await resolveOpenedNativeTab(tab.localId, opened);
    } catch (e) {
      const message = e instanceof Error ? e.message : "Failed to open diff";
      replaceNativeTab(tab.localId, (current) => nativeTabError(current, message));
    }
  }

  // Starts a restored live tab's first compute the moment it becomes the visible tab. A
  // no-op for anything not still pending — a regular native tab, or a live tab that's
  // already computed or already computing — so re-focusing never recomputes.
  function triggerLiveActivation(index: number): void {
    const tab = tabs[index];
    if (tab === undefined || tab.kind !== "native" || !isLiveTab(tab) || !pendingLiveActivation.has(tab.localId)) {
      return;
    }
    pendingLiveActivation.delete(tab.localId);
    void computeLiveTab(tab);
  }

  // Focusing a tab clears its freshness cue (the "new" accent dot from a just-opened batch)
  // and, for a restored live tab that hasn't computed yet, kicks off that first compute.
  function focusTab(index: number): void {
    active = index;
    tabs = clearTabFreshness(tabs, index);
    triggerLiveActivation(index);
  }

  // Test-only: focuses a restored live tab by its saved source path. A not-yet-computed live
  // tab has no `meta` yet, so `activateNativeRepo` (which matches on `meta.repo_name`) cannot
  // find it — this matches on the tab's saved source instead, then focuses through the same
  // `focusTab` a real click uses (freshness clearing + first-compute trigger included).
  function activateLiveTabBySource(sourceValue: string): boolean {
    const index = tabs.findIndex((tab) => isLiveTab(tab) && tab.source.value === sourceValue);
    if (index < 0) return false;
    focusTab(index);
    return true;
  }

  async function activateNativeRepo(repoName: string): Promise<boolean> {
    const index = tabs.findIndex((tab) => tab.kind === "native" && tab.meta?.repo_name === repoName);
    if (index < 0) return false;
    focusTab(index);
    flushSync();
    await tick();
    return true;
  }

  async function handleRefreshNativeTab(tab: Extract<ViewerTab, { kind: "native" }>): Promise<void> {
    const retry = beginNativeTabRetry(tab);
    if (retry === null) return;
    replaceNativeTab(tab.localId, () => retry.tab);

    try {
      if (retry.kind === "open") {
        const opened = await openRecipe(tab.recipe, tab.batchId);
        await resolveOpenedNativeTab(tab.localId, opened);
      } else {
        const meta = await refreshTab(retry.tabId);
        replaceNativeTab(tab.localId, (current) => nativeRefreshSucceeded(current, meta));
      }
    } catch (e) {
      const message =
        e instanceof Error ? e.message : retry.kind === "open" ? "Failed to open diff" : "Failed to refresh diff";
      replaceNativeTab(tab.localId, (current) => nativeTabError(current, message));
    }
  }

  function snapshotViewerState(): ReturnType<ViewerTestApi["snapshot"]> {
    const visible = activeTab;
    return {
      active,
      activeTab:
        visible === undefined
          ? null
          : {
              kind: "native" as const,
              tabId: visible.tabId,
              lifecycle: visible.lifecycle.state,
              repoName: visible.meta?.repo_name ?? null,
            },
      shellError,
      browserErrors: [...browserErrors],
      tabs: tabs.map((tab) => ({
        kind: "native" as const,
        localId: tab.localId,
        tabId: tab.tabId,
        batchId: tab.batchId,
        lifecycle: tab.lifecycle.state,
        repoName: tab.meta?.repo_name ?? null,
        fileCount: tab.meta?.files.length ?? null,
        live: isLiveTab(tab),
        source: isLiveTab(tab) ? tab.source : null,
        ...(tab.lifecycle.state === "error" ? { error: tab.lifecycle.message } : {}),
      })),
      toasts: toasts.map((toast) => ({ id: toast.id, message: toast.message })),
    };
  }

  // Restores persisted live views on mount. Restored tabs are pushed lazily: nothing is
  // computed here. `applyTabs` re-finds the caller-chosen focus key by identity after
  // sorting, so pushing tabs the user didn't ask for never steals focus — the user's
  // current tab (if any) stays active. Only when there was no active tab yet (a cold
  // start with nothing else queued) does the first restored tab become active, and its
  // compute starts immediately since it's now the tab on screen; every other restored
  // tab waits for `triggerLiveActivation` on first focus.
  async function restoreOnMount(): Promise<void> {
    try {
      const views = await listLiveViews();
      const restored: LiveTab[] = [];
      for (const view of views) {
        const tab = liveTabFromDto(view);
        if (dedupeLiveTabBySource(tabs, tab.source) === null && dedupeLiveTabBySource(restored, tab.source) === null) {
          restored.push(tab);
        }
      }

      const first = restored[0];
      if (first !== undefined) {
        const focusKey = activeTab === undefined ? viewerTabKey(first) : viewerTabKey(activeTab);
        applyTabs([...tabs, ...restored], focusKey);
        restored.forEach((tab) => pendingLiveActivation.add(tab.localId));
        triggerLiveActivation(active);
      }
    } catch (e) {
      shellError = e instanceof Error ? e.message : "Failed to restore live views";
    }
  }

  onMount(() => {
    const recordError = (value: unknown): void => {
      browserErrors.push(
        value instanceof Error ? `${value.name}: ${value.message}\n${value.stack ?? ""}` : String(value),
      );
    };
    const handleError = (event: ErrorEvent): void => recordError(event.error ?? event.message);
    const handleRejection = (event: PromiseRejectionEvent): void => recordError(event.reason);

    window.addEventListener("error", handleError);
    window.addEventListener("unhandledrejection", handleRejection);

    const testWindow = window as Window & { __GTL_VIEWER_TEST__?: ViewerTestApi };
    testWindow.__GTL_VIEWER_TEST__ = {
      openNativeRecipe,
      activateNativeRepo,
      announceBatch,
      openRecipeBatch,
      activateLiveTabBySource,
      snapshot: snapshotViewerState,
    };

    void restoreOnMount();
    drainPendingRecipes()
      .then((batches) => Promise.all(batches.map((batch) => openRecipeBatch(batch))))
      .catch((e: unknown) => {
        shellError = e instanceof Error ? e.message : "Failed to drain pending recipes";
      });
    listenOpenRecipe((batch) => {
      void openRecipeBatch(batch);
    })
      .then((stop) => {
        unlistenRecipe = stop;
      })
      .catch((e: unknown) => {
        shellError = e instanceof Error ? e.message : "Failed to subscribe to recipe events";
      });
    return () => {
      if (testWindow.__GTL_VIEWER_TEST__?.openNativeRecipe === openNativeRecipe) {
        delete testWindow.__GTL_VIEWER_TEST__;
      }
      window.removeEventListener("error", handleError);
      window.removeEventListener("unhandledrejection", handleRejection);
      unlistenRecipe?.();
      unlistenRecipe = undefined;
    };
  });
</script>

<div class="grid h-full grid-rows-[auto_1fr] bg-background text-foreground">
  <TabStrip
    {tabs}
    {active}
    onActivate={(i) => focusTab(i)}
    onClose={handleClose}
    onCloseOthers={handleCloseOthers}
    onCloseAll={handleCloseAll}
  />

  {#if shellError !== null}
    <div class="p-5 text-destructive">{shellError}</div>
  {:else if activeTab?.kind === "native"}
    {#key nativeViewKey(activeTab)}
      <DiffView tab={activeTab} onRefresh={handleRefreshNativeTab} />
    {/key}
  {:else}
    <div class="grid place-items-center text-sm text-foreground-muted">
      No diff open. Run <code class="mx-1.5 rounded bg-muted px-1.5 py-0.5 font-mono">gtl diff</code>.
    </div>
  {/if}
</div>
<Toast {toasts} ondismiss={dismissToast} />
