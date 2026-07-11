import type { DiffLayout } from "./file-panels";

/** Rendered height of a single diff row; the single source of truth for row sizing. */
export const ESTIMATED_ROW_HEIGHT_PX = 22;
/** Sticky panel header block (path line + badges line + border). Corrected by measurement. */
export const FILE_PANEL_HEADER_HEIGHT_PX = 52;
/** Trailing `pb-3` wrapper around each panel in the virtual list. */
export const FILE_PANEL_CHROME_PADDING_PX = 12;

/**
 * Hunk headers and surrounding context rows are not counted in a file's
 * added/removed metadata, so a metadata-only row estimate undershoots. ~40%
 * overhead is a defensible default for typical hunk density; the outer
 * virtualizer's `measureElement` corrects the panel once real rows render, so
 * this only affects first-paint scrollbar accuracy, never correctness.
 */
const HUNK_OVERHEAD_FACTOR = 1.4;

export type EstimateRowCountArgs = {
  readonly added: number;
  readonly removed: number;
  readonly layout: DiffLayout;
};

/** Estimate a file's rendered row count from its added/removed line metadata. */
export function estimateRowCount({ added, removed, layout }: EstimateRowCountArgs): number {
  const normalizedAdded = Math.max(0, Math.trunc(added));
  const normalizedRemoved = Math.max(0, Math.trunc(removed));
  // Split pairs old+new into one row where hunks align, but disjoint hunks
  // don't align — the midpoint between full alignment (max) and none (sum) is
  // the safe default given file metadata can't reveal per-hunk alignment.
  const changed =
    layout === "split" ? Math.ceil((normalizedAdded + normalizedRemoved) / 2) : normalizedAdded + normalizedRemoved;
  if (changed === 0) return 0;
  return Math.ceil(changed * HUNK_OVERHEAD_FACTOR);
}

export type EstimatePanelHeightArgs = EstimateRowCountArgs & {
  readonly collapsed: boolean;
};

/** Estimate a file panel's outer height (header + rows + chrome), or header-only when collapsed. */
export function estimatePanelHeight({ collapsed, ...rowArgs }: EstimatePanelHeightArgs): number {
  const chrome = FILE_PANEL_HEADER_HEIGHT_PX + FILE_PANEL_CHROME_PADDING_PX;
  if (collapsed) return chrome;
  return chrome + estimateRowCount(rowArgs) * ESTIMATED_ROW_HEIGHT_PX;
}
