import type { Tab } from "@/entities/tab";

export type FitResult = { visible: Tab[]; overflow: Tab[] };

/**
 * Decide which tabs fit. `reserved` covers the overflow trigger + right-side
 * buttons. The active tab is always forced into the visible set: if it would
 * overflow, it replaces the last visible slot and the displaced tab overflows.
 */
export function splitTabs(
  tabs: readonly Tab[],
  activeIndex: number,
  containerWidth: number,
  tabMinWidth: number,
  reserved: number,
): FitResult {
  const capacity = Math.max(1, Math.floor((containerWidth - reserved) / tabMinWidth));
  if (tabs.length <= capacity) return { visible: [...tabs], overflow: [] };

  const visible = tabs.slice(0, capacity) as Tab[];
  const overflow = tabs.slice(capacity) as Tab[];
  if (activeIndex >= capacity) {
    const displaced = visible[capacity - 1]!;
    visible[capacity - 1] = tabs[activeIndex]!;
    const idxInOverflow = overflow.findIndex((t) => t === tabs[activeIndex]);
    if (idxInOverflow >= 0) overflow.splice(idxInOverflow, 1, displaced);
  }
  return { visible, overflow };
}
