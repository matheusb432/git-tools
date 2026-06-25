// * Returns clamped next scrollLeft, or null to bail (ctrlKey zoom, no overflow, zero delta, already there).
export function computeWheelScroll(
  scroller: { readonly scrollWidth: number; readonly clientWidth: number; readonly scrollLeft: number },
  event: { readonly deltaX: number; readonly deltaY: number; readonly ctrlKey: boolean },
): number | null {
  const max = scroller.scrollWidth - scroller.clientWidth;
  if (max <= 0 || event.ctrlKey) return null;
  const delta = Math.abs(event.deltaY) >= Math.abs(event.deltaX) ? event.deltaY : event.deltaX;
  if (!delta) return null;
  const next = Math.max(0, Math.min(max, scroller.scrollLeft + delta));
  return next === scroller.scrollLeft ? null : next;
}
