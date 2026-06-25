<script lang="ts">
  import { onMount } from "svelte";
  import { groupHistoryByRepo, historyTabLabel as labelFromHistory } from "../core/history";
  import { closeTabState, type Tab } from "../core/tabs";
  import { drainPendingDiffs, listenOpenDiff, listHistory } from "./adapters/tauri";
  import type { HistoryEntry } from "./types";

  let tabs = $state<Tab[]>([]);
  let active = $state(0);
  let showHistory = $state(false);
  let history = $state<HistoryEntry[]>([]);
  let shellError = $state<string | null>(null);
  let unlisten: (() => void) | undefined;

  function labelFor(url: string): string {
    return url.replace("diff://", "").slice(0, 12);
  }

  function openTab(url: string, label = labelFor(url)): void {
    const existing = tabs.findIndex((tab) => tab.url === url);
    if (existing >= 0) {
      tabs = tabs.map((tab, index) => (index === existing && tab.label !== label ? { ...tab, label } : tab));
      active = existing;
      showHistory = false;
      return;
    }
    tabs = [...tabs, { url, label }];
    active = tabs.length - 1;
    showHistory = false;
  }

  function closeTab(index: number): void {
    const next = closeTabState({ tabs, active, showHistory }, index);
    tabs = [...next.tabs];
    active = next.active;
    showHistory = next.showHistory;
  }

  async function openHistory(): Promise<void> {
    try {
      history = await listHistory();
      showHistory = true;
      shellError = null;
    } catch (error) {
      shellError = error instanceof Error ? error.message : "Failed to load history";
    }
  }

  onMount(() => {
    drainPendingDiffs()
      .then((urls) => urls.forEach((url) => openTab(url)))
      .catch((error: unknown) => {
        shellError = error instanceof Error ? error.message : "Failed to drain pending diffs";
      });
    listenOpenDiff((url) => openTab(url))
      .then((stop) => {
        unlisten = stop;
      })
      .catch((error: unknown) => {
        shellError = error instanceof Error ? error.message : "Failed to subscribe to diff events";
      });
    return () => {
      unlisten?.();
      unlisten = undefined;
    };
  });
</script>

<div class="shell">
  <div class="tabs">
    {#each tabs as tab, i}
      <div
        class="tab"
        class:active={!showHistory && i === active}
        onclick={() => { active = i; showHistory = false; }}
        title={tab.label}
        role="tab"
        tabindex="0"
        onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { active = i; showHistory = false; } }}
      >
        <span class="tab-label">{tab.label}</span>
        <button
          class="tab-close"
          type="button"
          aria-label="Close {tab.label}"
          title="Close tab"
          onclick={(e) => { e.stopPropagation(); closeTab(i); }}
        >×</button>
      </div>
    {/each}
    <div
      class="tab history"
      class:active={showHistory}
      onclick={() => openHistory()}
      role="tab"
      tabindex="0"
      onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") openHistory(); }}
    >
      History
    </div>
  </div>

  {#if shellError !== null}
    <div class="panel error">{shellError}</div>
  {:else if showHistory}
    <div class="panel">
      {#each [...groupHistoryByRepo(history).entries()] as [repo, rows]}
        <div class="repo">{repo}</div>
        {#each rows as row}
          <div
            class="row"
            onclick={() => openTab(row.url, labelFromHistory(row, labelFor))}
            role="button"
            tabindex="0"
            onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") openTab(row.url, labelFromHistory(row, labelFor)); }}
          >
            {row.title} · <small>{row.range_label} · {row.head_committed_at}</small>
          </div>
        {/each}
      {/each}
    </div>
  {:else}
    {#if tabs[active] !== undefined}
      <iframe src={tabs[active]?.url} title="diff preview"></iframe>
    {:else}
      <div class="panel">
        No diff open. Run <code>gtl diff</code> or pick from History.
      </div>
    {/if}
  {/if}
</div>

<style>
  .shell {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100%;
    background: #1e1e1e;
    color: #d4d4d4;
    font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto,
      "Helvetica Neue", Arial, sans-serif;
    font-size: 14px;
    -webkit-font-smoothing: antialiased;
  }
  .tabs {
    display: flex;
    gap: 2px;
    background: #181818;
    padding: 6px 8px 0;
    overflow-x: auto;
    border-bottom: 1px solid #2d2d2d;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 7px 8px 7px 14px;
    color: #9d9d9d;
    background: #232323;
    border-radius: 7px 7px 0 0;
    cursor: pointer;
    white-space: nowrap;
    font-family: ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace;
    font-size: 12.5px;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .tab:hover { color: #d4d4d4; }
  .tab.active { background: #1e1e1e; color: #fff; box-shadow: inset 0 2px 0 #569cd6; }
  .tab-label {
    max-width: 22ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tab-close {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    color: #858585;
    background: transparent;
    cursor: pointer;
    font: inherit;
    line-height: 1;
  }
  .tab-close:hover {
    color: #fff;
    background: #3a3d3f;
  }
  .tab.history { margin-left: auto; font-family: inherit; font-weight: 600; }
  iframe { border: 0; width: 100%; height: 100%; background: #1e1e1e; }
  .panel { padding: 18px 22px; overflow: auto; }
  .panel code {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    background: #2a2a2a;
    padding: 1px 6px;
    border-radius: 4px;
  }
  .panel.error { color: #f48771; }
  .repo {
    margin: 20px 0 8px;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #569cd6;
  }
  .repo:first-child { margin-top: 0; }
  .row {
    padding: 9px 12px;
    cursor: pointer;
    border-radius: 6px;
    border: 1px solid transparent;
    line-height: 1.5;
  }
  .row:hover { background: #2a2d2e; border-color: #333; }
  .row small {
    color: #858585;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
  }
</style>
