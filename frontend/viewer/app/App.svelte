<script lang="ts">
  import { onMount } from "svelte";
  import { TabStrip } from "@/widgets/tab-strip";
  import { HistoryPanel } from "@/widgets/history-panel";
  import { closeAllTabs, closeOthers, closeTabState, type Tab } from "@/entities/tab";
  import { labelMap, type HistoryEntry } from "@/entities/diff";
  import { drainPendingDiffs, listenOpenDiff, listHistory } from "@/shared/api";

  let tabs = $state<Tab[]>([]);
  let active = $state(0);
  let showHistory = $state(false);
  let history = $state<HistoryEntry[]>([]);
  let shellError = $state<string | null>(null);
  let labels = new Map<string, string>();
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
  }

  function openTab(url: string, label?: string): void {
    const resolved = label ?? labels.get(url) ?? hashLabel(url);
    const existing = tabs.findIndex((t) => t.url === url);
    if (existing >= 0) {
      const tab = tabs[existing];
      if (tab !== undefined && tab.label !== resolved) tabs[existing] = { url, label: resolved };
      active = existing;
      showHistory = false;
      return;
    }
    tabs = [...tabs, { url, label: resolved }];
    active = tabs.length - 1;
    showHistory = false;
    // Brand-new diff not yet in the label cache: resolve from the store, then relabel.
    if (label === undefined && !labels.has(url)) {
      void refreshHistory().then(() => {
        const found = labels.get(url);
        const i = tabs.findIndex((t) => t.url === url);
        const tab = tabs[i];
        if (found !== undefined && i >= 0 && tab !== undefined && tab.label !== found) {
          tabs[i] = { url, label: found };
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
