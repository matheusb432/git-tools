export type Tab = { readonly url: string; readonly label: string };

export type TabState = {
  readonly tabs: readonly Tab[];
  readonly active: number;
  readonly showHistory: boolean;
};

export function closeTabState(state: TabState, index: number): TabState {
  if (index < 0 || index >= state.tabs.length) {
    return state;
  }

  const tabs = state.tabs.filter((_, i) => i !== index);
  if (tabs.length === 0) {
    return { tabs, active: 0, showHistory: state.showHistory };
  }

  const active = nextActiveIndex(state.active, index, tabs.length);
  return { tabs, active, showHistory: state.showHistory };
}

function nextActiveIndex(active: number, closed: number, remaining: number): number {
  if (closed < active) {
    return active - 1;
  }
  if (closed === active) {
    return Math.min(closed, remaining - 1);
  }
  return Math.min(active, remaining - 1);
}
