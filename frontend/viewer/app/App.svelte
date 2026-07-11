<script lang="ts">
  import { onMount } from "svelte";
  import { TabStrip } from "@/widgets/tab-strip";
  import { HistoryPanel } from "@/widgets/history-panel";
  import { labelMap, timestampMap, type HistoryEntry } from "@/entities/diff";
  import { drainPendingDiffs, listenOpenDiff, listHistory } from "@/shared/api";
  import {
    closeAllViewerTabs,
    closeOtherViewerTabs,
    closeViewerTabState,
    sortViewerTabsByTime,
    viewerTabKey,
    type ArtifactTab,
    type ViewerTab,
    type ViewerTabState,
  } from "@/entities/diff-tab";

  type ViewerTestApi = {
    snapshot: () => {
      readonly active: number;
      readonly activeTab: { readonly kind: ViewerTab["kind"] } | null;
      readonly showHistory: boolean;
      readonly shellError: string | null;
      readonly browserErrors: readonly string[];
      readonly tabs: readonly { readonly kind: ViewerTab["kind"] }[];
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

  function handleClose(index: number): void {
    apply(closeViewerTabState({ tabs, active, showHistory }, index));
  }

  function handleCloseOthers(index: number): void {
    apply(closeOtherViewerTabs({ tabs, active, showHistory }, index));
  }

  function handleCloseAll(): void {
    apply(closeAllViewerTabs({ tabs, active, showHistory }));
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

  function snapshotViewerState(): ReturnType<ViewerTestApi["snapshot"]> {
    const visible = activeTab;
    return {
      active,
      activeTab: visible === undefined ? null : { kind: "artifact" as const },
      showHistory,
      shellError,
      browserErrors: [...browserErrors],
      tabs: tabs.map(() => ({ kind: "artifact" as const })),
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
      if (testWindow.__GTL_VIEWER_TEST__?.snapshot === snapshotViewerState) {
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
  {:else if activeTab?.kind === "artifact"}
    <iframe src={activeTab.url} title="diff preview" class="h-full w-full border-0 bg-background"></iframe>
  {:else}
    <div class="grid place-items-center text-sm text-foreground-muted">
      No diff open. Run <code class="mx-1.5 rounded bg-muted px-1.5 py-0.5 font-mono">gtl diff</code> or pick from History.
    </div>
  {/if}
</div>
