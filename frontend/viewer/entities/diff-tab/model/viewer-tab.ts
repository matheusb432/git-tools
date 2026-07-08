import type { OpenedTab } from "@/shared/api";
import { nativeReadyTab, type ViewerTab } from "./native-tab";

export type ViewerTabState = {
  readonly tabs: readonly ViewerTab[];
  readonly active: number;
  readonly showHistory: boolean;
};

export type ApplyOpenedNativeTabResult = {
  readonly next: ViewerTabState;
  readonly openedTabIdToClose: number | null;
};

export function viewerTabKey(tab: ViewerTab): string {
  return tab.kind === "artifact" ? `artifact:${tab.url}` : `native:${tab.localId}`;
}

/** Most recently updated viewer tab first. Stable sort keeps ties in their prior relative order. */
export function sortViewerTabsByTime(tabs: readonly ViewerTab[]): readonly ViewerTab[] {
  return [...tabs]
    .map((tab, index) => ({ tab, index }))
    .sort((a, b) => {
      const aKey = viewerTabSortKey(a.tab);
      const bKey = viewerTabSortKey(b.tab);
      if (aKey < bKey) return 1;
      if (aKey > bKey) return -1;
      return a.index - b.index;
    })
    .map(({ tab }) => tab);
}

export function closeViewerTabState(state: ViewerTabState, index: number): ViewerTabState {
  if (index < 0 || index >= state.tabs.length) return state;
  const tabs = state.tabs.filter((_, i) => i !== index);
  if (tabs.length === 0) return { tabs, active: 0, showHistory: state.showHistory };
  return { tabs, active: nextActiveIndex(state.active, index, tabs.length), showHistory: state.showHistory };
}

export function closeAllViewerTabs(state: ViewerTabState): ViewerTabState {
  return { tabs: [], active: 0, showHistory: state.showHistory };
}

export function closeOtherViewerTabs(state: ViewerTabState, keep: number): ViewerTabState {
  const kept = state.tabs[keep];
  if (kept === undefined) return state;
  return { tabs: [kept], active: 0, showHistory: state.showHistory };
}

export function applyOpenedNativeTab(
  state: ViewerTabState,
  localId: string,
  opened: OpenedTab,
): ApplyOpenedNativeTabResult {
  const focusKey = `native:${localId}`;
  let found = false;
  const nextTabs = sortViewerTabsByTime(
    state.tabs.map((tab) => {
      if (tab.kind !== "native" || tab.localId !== localId) return tab;
      found = true;
      return nativeReadyTab(tab, opened);
    }),
  );

  if (!found) {
    return { next: state, openedTabIdToClose: opened.tab_id };
  }

  const active = nextTabs.findIndex((tab) => viewerTabKey(tab) === focusKey);
  return {
    next: {
      tabs: nextTabs,
      active: active >= 0 ? active : Math.min(state.active, Math.max(nextTabs.length - 1, 0)),
      showHistory: state.showHistory,
    },
    openedTabIdToClose: null,
  };
}

function nextActiveIndex(active: number, closed: number, remaining: number): number {
  if (closed < active) return active - 1;
  if (closed === active) return Math.min(closed, remaining - 1);
  return Math.min(active, remaining - 1);
}

function viewerTabSortKey(tab: ViewerTab): string {
  if (tab.kind === "artifact") return tab.committedAt;
  return tab.meta?.commits[0]?.iso ?? tab.batchId;
}
