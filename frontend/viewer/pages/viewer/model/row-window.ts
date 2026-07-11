/** The mounted row slice re-anchors in strides of this many rows, adding hysteresis against flicker. */
export const ROW_WINDOW_STRIDE = 20;
/** Extra rows (in px) mounted above and below the viewport so scrolling reveals no blank gap. */
export const ROW_WINDOW_OVERSCAN_PX = 600;

export type RowWindowArgs = {
  /** Scroll offset of the single outer scroll surface. */
  readonly outerScrollTop: number;
  /** Height of the outer viewport. */
  readonly outerViewportHeight: number;
  /** This panel's top offset within the outer scroll content (the virtualizer's `item.start`). */
  readonly panelTop: number;
  /** Height of the panel's sticky header, above its first row. */
  readonly headerHeight: number;
  /** Total rows in the file — real once known, else the metadata estimate. */
  readonly totalRows: number;
  /** Estimated per-row height. */
  readonly rowHeight: number;
  readonly overscanPx?: number;
};

export type RowWindowRange = {
  readonly start: number;
  readonly end: number;
};

/**
 * Which rows of a single file panel fall near the shared outer viewport.
 *
 * Projects the outer viewport (± overscan) into the panel's local row-index
 * space and returns the clipped, stride-quantized `[start, end]` range, or
 * `null` when the panel's rows are entirely outside the overscanned band (so
 * far-off panels mount zero rows — the mechanism that bounds DOM without an
 * inner scroll surface). The stride quantization mirrors `filePanelWindowIndexes`:
 * the mounted slice only reshuffles every `ROW_WINDOW_STRIDE` rows, not on every
 * pixel of scroll.
 */
export function visibleRowWindow(args: RowWindowArgs): RowWindowRange | null {
  const total = Math.max(0, Math.trunc(args.totalRows));
  if (total === 0) return null;

  const rowHeight = Math.max(1, args.rowHeight);
  const overscan = Math.max(0, args.overscanPx ?? ROW_WINDOW_OVERSCAN_PX);
  const rowsTop = args.panelTop + args.headerHeight;
  const bandTop = args.outerScrollTop - overscan;
  const bandBottom = args.outerScrollTop + args.outerViewportHeight + overscan;

  let start = Math.floor((bandTop - rowsTop) / rowHeight);
  let end = Math.ceil((bandBottom - rowsTop) / rowHeight) - 1;
  if (end < 0 || start > total - 1) return null;

  start = Math.max(0, start);
  end = Math.min(total - 1, end);
  if (start > end) return null;

  const stride = Math.max(1, ROW_WINDOW_STRIDE);
  start = Math.floor(start / stride) * stride;
  end = Math.min(total - 1, Math.ceil((end + 1) / stride) * stride - 1);
  return { start, end };
}
