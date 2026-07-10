/** Minimum number of file panels kept mounted so scrolling never reveals a blank gap. */
export const FILE_PANEL_MIN_WINDOW = 20;
/** The rendered window only shifts in strides of this many files, adding hysteresis against flicker. */
export const FILE_PANEL_WINDOW_STEP = 5;

export type FilePanelIdentity = {
  readonly fileIdx: number;
  readonly path: string;
};

export type FilePanelRange = {
  readonly startIndex: number;
  readonly endIndex: number;
  readonly count: number;
};

export function filePanelDomKey(file: FilePanelIdentity): string {
  return `${file.fileIdx}:${file.path}`;
}

function clampIndex(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}

/**
 * Range extractor for the file-panel virtualizer.
 *
 * Keeps at least `minWindow` panels mounted and only re-anchors the rendered
 * window on `step`-file boundaries. The stride quantization is deliberate: it
 * absorbs both scroll jitter and the position jitter of dynamic panel
 * measurement, so the mounted set stays stable instead of remounting (and
 * refetching) panels on every pixel of scroll.
 */
export function filePanelWindowIndexes(
  range: FilePanelRange,
  minWindow: number = FILE_PANEL_MIN_WINDOW,
  step: number = FILE_PANEL_WINDOW_STEP,
): number[] {
  const count = Math.max(0, Math.trunc(range.count));
  if (count === 0) return [];

  const stride = Math.max(1, Math.trunc(step));
  const window = Math.min(count, Math.max(1, Math.trunc(minWindow)));
  const visibleStart = clampIndex(range.startIndex, 0, count - 1);
  const visibleEnd = clampIndex(range.endIndex, visibleStart, count - 1);

  // Span covers the visible range but never renders fewer than `window` panels,
  // rounded up to a stride multiple so both edges move in whole strides.
  const rawSpan = Math.max(window, visibleEnd - visibleStart + 1);
  const span = Math.min(count, Math.ceil(rawSpan / stride) * stride);

  // Anchor the window start just below the first visible panel, quantized down
  // to a stride boundary (the hysteresis), then nudge forward in strides only
  // if the span would fail to cover the last visible panel.
  let start = Math.floor(visibleStart / stride) * stride;
  while (start + span - 1 < visibleEnd) start += stride;

  let end = start + span - 1;
  if (end > count - 1) {
    end = count - 1;
    start = Math.floor(Math.max(0, end - span + 1) / stride) * stride;
  }
  start = Math.max(0, start);

  const length = end - start + 1;
  const indexes = new Array<number>(length);
  for (let offset = 0; offset < length; offset += 1) indexes[offset] = start + offset;
  return indexes;
}
