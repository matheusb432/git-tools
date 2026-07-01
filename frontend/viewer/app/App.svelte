<script lang="ts">
  import { onMount } from "svelte";
  import { TabStrip } from "@/widgets/tab-strip";
  import { HistoryPanel } from "@/widgets/history-panel";
  import { closeAllTabs, closeOthers, closeTabState, sortTabsByTime, type Tab } from "@/entities/tab";
  import { labelMap, timestampMap, type HistoryEntry } from "@/entities/diff";
  import { drainPendingDiffs, listenOpenDiff, listHistory } from "@/shared/api";

  let tabs = $state<Tab[]>([]);
  let active = $state(0);
  let showHistory = $state(false);
  let history = $state<HistoryEntry[]>([]);
  let shellError = $state<string | null>(null);
  let labels = new Map<string, string>();
  let timestamps = new Map<string, string>();
  let unlisten: (() => void) | undefined;

  let now = $state(new Date());

  // Extract the content-hash segment (last path component) so every unnamed diff
  // in the same repo gets a distinct 12-char prefix rather than reusing the repo_id.
  const hashLabel = (url: string) => url.split("/").pop()?.slice(0, 12) ?? url;

  // Reactive derived for the currently visible tab; narrows to Tab | undefined so
  // the template can narrow it cleanly without array-index unsafety.
  const activeTab = $derived<Tab | undefined>(tabs[active]);

  async function refreshHistory(): Promise<void> {
    history = await listHistory();
    labels = labelMap(history, hashLabel);
    timestamps = timestampMap(history);
  }

  // Re-sorts tabs newest-committed-first, then makes `focusUrl`'s tab active — so a
  // background relabel/retime can reorder the strip without stealing the user's focus,
  // by passing the *currently* active tab's url instead of the one that just changed.
  function applyTabs(next: Tab[], focusUrl: string): void {
    tabs = [...sortTabsByTime(next)];
    const i = tabs.findIndex((t) => t.url === focusUrl);
    if (i >= 0) active = i;
  }

  function openTab(url: string, label?: string): void {
    const resolvedLabel = label ?? labels.get(url) ?? hashLabel(url);
    const existing = tabs.find((t) => t.url === url);
    const committedAt = timestamps.get(url) ?? existing?.committedAt ?? new Date().toISOString();

    if (existing === undefined || existing.label !== resolvedLabel || existing.committedAt !== committedAt) {
      const next =
        existing === undefined
          ? [...tabs, { url, label: resolvedLabel, committedAt }]
          : tabs.map((t) => (t.url === url ? { url, label: resolvedLabel, committedAt } : t));
      applyTabs(next, url);
    } else {
      active = tabs.findIndex((t) => t.url === url);
    }
    showHistory = false;

    // Brand-new diff not yet in the label/timestamp cache: resolve from the store, then
    // relabel/retime in place. Focus stays on whatever tab is currently active — this is
    // a background refresh, not a user-initiated open.
    if (existing === undefined && label === undefined && !labels.has(url)) {
      void refreshHistory().then(() => {
        const found = labels.get(url);
        const foundTime = timestamps.get(url);
        const current = tabs.find((t) => t.url === url);
        if (
          current !== undefined &&
          ((found !== undefined && current.label !== found) || (foundTime !== undefined && current.committedAt !== foundTime))
        ) {
          const focusUrl = tabs[active]?.url ?? url;
          applyTabs(
            tabs.map((t) =>
              t.url === url ? { url, label: found ?? t.label, committedAt: foundTime ?? t.committedAt } : t,
            ),
            focusUrl,
          );
        }
      });
    }
  }

  function apply(next: { tabs: readonly Tab[]; active: number; showHistory: boolean }): void {
    tabs = [...next.tabs];
    active = next.active;
    showHistory = next.showHistory;
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

  onMount(() => {
    refreshHistory().catch(() => { /* history is best-effort; tabs still work */ });
    drainPendingDiffs()
      .then((urls) => urls.forEach((u) => openTab(u)))
      .catch((e: unknown) => { shellError = e instanceof Error ? e.message : "Failed to drain pending diffs"; });
    listenOpenDiff((u) => openTab(u))
      .then((stop) => { unlisten = stop; })
      .catch((e: unknown) => { shellError = e instanceof Error ? e.message : "Failed to subscribe to diff events"; });
    return () => { unlisten?.(); unlisten = undefined; };
  });
</script>

<div class="grid grid-rows-[auto_1fr] h-full bg-background text-foreground">
  <TabStrip
    {tabs} {active} {showHistory}
    onActivate={(i) => { active = i; showHistory = false; }}
    onClose={(i) => apply(closeTabState({ tabs, active, showHistory }, i))}
    onCloseOthers={(i) => apply(closeOthers({ tabs, active, showHistory }, i))}
    onCloseAll={() => apply(closeAllTabs({ tabs, active, showHistory }))}
    onOpenHistory={openHistory}
  />

  {#if shellError !== null}
    <div class="p-5 text-destructive">{shellError}</div>
  {:else if showHistory}
    <HistoryPanel {history} {now} onOpen={openTab} />
  {:else if activeTab !== undefined}
    <iframe src={activeTab.url} title="diff preview" class="w-full h-full border-0 bg-background"></iframe>
  {:else}
    <div class="grid place-items-center text-foreground-muted text-sm">
      No diff open. Run <code class="mx-1.5 font-mono bg-muted px-1.5 py-0.5 rounded">gtl diff</code> or pick from History.
    </div>
  {/if}
</div>
