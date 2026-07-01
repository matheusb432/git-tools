export type Tab = { readonly url: string; readonly label: string; readonly committedAt: string };

/** Most recently committed diff first. Stable sort keeps ties in their prior relative order. */
export function sortTabsByTime(tabs: readonly Tab[]): readonly Tab[] {
  return [...tabs].sort((a, b) => (a.committedAt < b.committedAt ? 1 : a.committedAt > b.committedAt ? -1 : 0));
}

export type TabState = {
  readonly tabs: readonly Tab[];
  readonly active: number;
  readonly showHistory: boolean;
};

export function closeTabState(state: TabState, index: number): TabState {
  if (index < 0 || index >= state.tabs.length) return state;
  const tabs = state.tabs.filter((_, i) => i !== index);
  if (tabs.length === 0) return { tabs, active: 0, showHistory: state.showHistory };
  return { tabs, active: nextActiveIndex(state.active, index, tabs.length), showHistory: state.showHistory };
}

export function closeAllTabs(state: TabState): TabState {
  return { tabs: [], active: 0, showHistory: state.showHistory };
}

export function closeOthers(state: TabState, keep: number): TabState {
  const kept = state.tabs[keep];
  if (kept === undefined) return state;
  return { tabs: [kept], active: 0, showHistory: state.showHistory };
}

function nextActiveIndex(active: number, closed: number, remaining: number): number {
  if (closed < active) return active - 1;
  if (closed === active) return Math.min(closed, remaining - 1);
  return Math.min(active, remaining - 1);
}
