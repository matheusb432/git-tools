<script lang="ts">
  import { flushSync, onMount, tick } from "svelte";
  import { DiffView } from "@/widgets/diff-view";
  import { TabStrip } from "@/widgets/tab-strip";
  import { HistoryPanel } from "@/widgets/history-panel";
  import { labelMap, timestampMap, type HistoryEntry } from "@/entities/diff";
  import {
    closeNativeTab,
    drainPendingDiffs,
    listenOpenDiff,
    listHistory,
    openRecipe,
    refreshTab,
    type Recipe,
  } from "@/shared/api";
  import {
    beginNativeTabRetry,
    applyOpenedNativeTab,
    closeAllViewerTabs,
    closeOtherViewerTabs,
    closeViewerTabState,
    nativeRefreshSucceeded,
    nativeOpeningTab,
    nativeTabError,
    sortViewerTabsByTime,
    viewerTabKey,
    type ArtifactTab,
    type NativeTab,
    type ViewerTab,
    type ViewerTabState,
  } from "@/entities/diff-tab";

  type ViewerTestApi = {
    openNativeRecipe: (recipe: Recipe, batchId?: string) => Promise<void>;
    activateNativeRepo: (repoName: string) => Promise<boolean>;
    snapshot: () => {
      readonly active: number;
      readonly activeTab: {
        readonly kind: ViewerTab["kind"];
        readonly tabId?: number | null;
        readonly lifecycle?: NativeTab["lifecycle"]["state"];
        readonly repoName?: string | null;
      } | null;
      readonly showHistory: boolean;
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
      }[];
    };
  };

  let tabs = $state<ViewerTab[]>([]);
  let active = $state(0);
  let showHistory = $state(false);
  let history = $state<HistoryEntry[]>([]);
  let shellError = $state<string | null>(null);
  let labels = new Map<string, string>();
  let timestamps = new Map<string, string>();
  let unlisten: (() => void) | undefined;

  let now = $state(new Date());
  const browserErrors: string[] = [];

  // Extract the content-hash segment (last path component) so every unnamed diff
  // in the same repo gets a distinct 12-char prefix rather than reusing the repo_id.
  const hashLabel = (url: string) => url.split("/").pop()?.slice(0, 12) ?? url;
  const nativeViewKey = (tab: Extract<ViewerTab, { kind: "native" }>) =>
    `${tab.localId}:${tab.batchId}:${tab.tabId ?? "opening"}`;

  // Reactive derived for the currently visible tab; narrows to Tab | undefined so
  // the template can narrow it cleanly without array-index unsafety.
  const activeTab = $derived<ViewerTab | undefined>(tabs[active]);

  async function refreshHistory(): Promise<void> {
    history = await listHistory();
    labels = labelMap(history, hashLabel);
    timestamps = timestampMap(history);
  }

  // Re-sorts tabs newest-first, then makes `focusKey`'s tab active — so a background
  // relabel/retime can reorder the strip without stealing the user's focus.
  function applyTabs(next: readonly ViewerTab[], focusKey: string): void {
    tabs = [...sortViewerTabsByTime(next)];
    const i = tabs.findIndex((tab) => viewerTabKey(tab) === focusKey);
    active = i >= 0 ? i : Math.min(active, Math.max(tabs.length - 1, 0));
  }

  function openTab(url: string, label?: string): void {
    const resolvedLabel = label ?? labels.get(url) ?? hashLabel(url);
    const existing = tabs.find((tab): tab is ArtifactTab => tab.kind === "artifact" && tab.url === url);
    const committedAt = timestamps.get(url) ?? existing?.committedAt ?? new Date().toISOString();

    if (existing === undefined || existing.label !== resolvedLabel || existing.committedAt !== committedAt) {
      const next: readonly ViewerTab[] =
        existing === undefined
          ? [...tabs, { kind: "artifact", url, label: resolvedLabel, committedAt }]
          : tabs.map((tab) =>
              tab.kind === "artifact" && tab.url === url
                ? { kind: "artifact", url, label: resolvedLabel, committedAt }
                : tab,
            );
      applyTabs(next, `artifact:${url}`);
    } else {
      active = tabs.findIndex((tab) => tab.kind === "artifact" && tab.url === url);
    }
    showHistory = false;

    // Brand-new diff not yet in the label/timestamp cache: resolve from the store, then
    // relabel/retime in place. Focus stays on whatever tab is currently active — this is
    // a background refresh, not a user-initiated open.
    if (existing === undefined && label === undefined && !labels.has(url)) {
      void refreshHistory().then(() => {
        const found = labels.get(url);
        const foundTime = timestamps.get(url);
        const current = tabs.find((tab): tab is ArtifactTab => tab.kind === "artifact" && tab.url === url);
        if (
          current !== undefined &&
          ((found !== undefined && current.label !== found) ||
            (foundTime !== undefined && current.committedAt !== foundTime))
        ) {
          const focusKey = activeTab === undefined ? `artifact:${url}` : viewerTabKey(activeTab);
          applyTabs(
            tabs.map((tab) =>
              tab.kind === "artifact" && tab.url === url
                ? { kind: "artifact", url, label: found ?? tab.label, committedAt: foundTime ?? tab.committedAt }
                : tab,
            ),
            focusKey,
          );
        }
      });
    }
  }

  function apply(next: ViewerTabState): void {
    tabs = [...next.tabs];
    active = next.active;
    showHistory = next.showHistory;
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
    const { next, openedTabIdToClose } = applyOpenedNativeTab({ tabs, active, showHistory }, localId, opened);
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
    apply(closeViewerTabState({ tabs, active, showHistory }, index));
    await closeNativeTabs([tab]);
  }

  async function handleCloseOthers(index: number): Promise<void> {
    const keep = tabs[index];
    if (keep === undefined) return;
    const toClose = tabs.filter((_, tabIndex) => tabIndex !== index);
    apply(closeOtherViewerTabs({ tabs, active, showHistory }, index));
    await closeNativeTabs(toClose);
  }

  async function handleCloseAll(): Promise<void> {
    const toClose = [...tabs];
    apply(closeAllViewerTabs({ tabs, active, showHistory }));
    await closeNativeTabs(toClose);
  }

  async function openHistory(): Promise<void> {
    try {
      now = new Date();
      await refreshHistory();
      showHistory = true;
      shellError = null;
    } catch (e) {
      shellError = e instanceof Error ? e.message : "Failed to load history";
    }
  }

  async function openNativeRecipe(recipe: Recipe, batchId: string = crypto.randomUUID()): Promise<void> {
    const localId = crypto.randomUUID();
    const opening = nativeOpeningTab(localId, recipe, batchId);
    applyTabs([...tabs, opening], viewerTabKey(opening));
    showHistory = false;
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

    showHistory = false;
    flushSync();
    await tick();
  }

  async function activateNativeRepo(repoName: string): Promise<boolean> {
    const index = tabs.findIndex((tab) => tab.kind === "native" && tab.meta?.repo_name === repoName);
    if (index < 0) return false;
    active = index;
    showHistory = false;
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
          : visible.kind === "artifact"
            ? { kind: "artifact" as const }
            : {
                kind: "native" as const,
                tabId: visible.tabId,
                lifecycle: visible.lifecycle.state,
                repoName: visible.meta?.repo_name ?? null,
              },
      showHistory,
      shellError,
      browserErrors: [...browserErrors],
      tabs: tabs.map((tab) =>
        tab.kind === "artifact"
          ? { kind: "artifact" as const }
          : {
              kind: "native" as const,
              localId: tab.localId,
              tabId: tab.tabId,
              batchId: tab.batchId,
              lifecycle: tab.lifecycle.state,
              repoName: tab.meta?.repo_name ?? null,
              fileCount: tab.meta?.files.length ?? null,
              ...(tab.lifecycle.state === "error" ? { error: tab.lifecycle.message } : {}),
            },
      ),
    };
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
      snapshot: snapshotViewerState,
    };

    refreshHistory().catch(() => {
      /* history is best-effort; tabs still work */
    });
    drainPendingDiffs()
      .then((urls) => urls.forEach((u) => openTab(u)))
      .catch((e: unknown) => {
        shellError = e instanceof Error ? e.message : "Failed to drain pending diffs";
      });
    listenOpenDiff((u) => openTab(u))
      .then((stop) => {
        unlisten = stop;
      })
      .catch((e: unknown) => {
        shellError = e instanceof Error ? e.message : "Failed to subscribe to diff events";
      });
    return () => {
      if (testWindow.__GTL_VIEWER_TEST__?.openNativeRecipe === openNativeRecipe) {
        delete testWindow.__GTL_VIEWER_TEST__;
      }
      window.removeEventListener("error", handleError);
      window.removeEventListener("unhandledrejection", handleRejection);
      unlisten?.();
      unlisten = undefined;
    };
  });
</script>

<div class="grid h-full grid-rows-[auto_1fr] bg-background text-foreground">
  <TabStrip
    {tabs}
    {active}
    {showHistory}
    onActivate={(i) => {
      active = i;
      showHistory = false;
    }}
    onClose={handleClose}
    onCloseOthers={handleCloseOthers}
    onCloseAll={handleCloseAll}
    onOpenHistory={openHistory}
  />

  {#if shellError !== null}
    <div class="p-5 text-destructive">{shellError}</div>
  {:else if showHistory}
    <HistoryPanel {history} {now} onOpen={openTab} />
  {:else if activeTab?.kind === "native"}
    {#key nativeViewKey(activeTab)}
      <DiffView tab={activeTab} onRefresh={handleRefreshNativeTab} />
    {/key}
  {:else if activeTab?.kind === "artifact"}
    <iframe src={activeTab.url} title="diff preview" class="h-full w-full border-0 bg-background"></iframe>
  {:else}
    <div class="grid place-items-center text-sm text-foreground-muted">
      No diff open. Run <code class="mx-1.5 rounded bg-muted px-1.5 py-0.5 font-mono">gtl diff</code> or pick from History.
    </div>
  {/if}
</div>
