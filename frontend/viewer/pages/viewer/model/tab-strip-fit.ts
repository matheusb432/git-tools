type IdentifiedTab = { readonly localId: string };

export type FitResult<T extends IdentifiedTab> = { visible: T[]; overflow: T[] };

export function viewerTabKey(tab: IdentifiedTab): string {
  return `native:${tab.localId}`;
}

/**
 * Decide which tabs fit. `reserved` covers the overflow trigger + right-side
 * buttons. The active tab is always forced into the visible set: if it would
 * overflow, it replaces the last visible slot and the displaced tab overflows.
 */
export function splitTabs<T extends IdentifiedTab>(
  tabs: readonly T[],
  activeIndex: number,
  containerWidth: number,
  tabMinWidth: number,
  reserved: number,
): FitResult<T> {
  const capacity = Math.max(1, Math.floor((containerWidth - reserved) / tabMinWidth));
  if (tabs.length <= capacity) return { visible: [...tabs], overflow: [] };

  const visible = tabs.slice(0, capacity);
  const overflow = tabs.slice(capacity);
  if (activeIndex >= capacity) {
    const displaced = visible[capacity - 1];
    const activeTab = tabs[activeIndex];
    if (displaced === undefined || activeTab === undefined) return { visible, overflow };
    visible[capacity - 1] = activeTab;
    const idxInOverflow = overflow.findIndex((tab) => tab.localId === activeTab.localId);
    if (idxInOverflow >= 0) overflow.splice(idxInOverflow, 1, displaced);
  }
  return { visible, overflow };
}
