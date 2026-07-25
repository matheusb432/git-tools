export type ScrollLandOptions = {
  readonly stickyTop: number;
  readonly raf?: (cb: FrameRequestCallback) => number;
  readonly maxFrames?: number;
};

type Measurable = { getBoundingClientRect(): { readonly top: number } };
type ScrollContainer = Measurable & { scrollTop: number };

/**
 * Lands scroll exactly on `target`: content-visibility boxes realize their true height
 * mid-scroll and shift it, so re-measure and re-align each frame until stable.
 */
export function scrollLandOn(target: Measurable, scroller: ScrollContainer, opts: ScrollLandOptions): void {
  const raf = opts.raf ?? ((cb) => requestAnimationFrame(cb));
  const maxFrames = opts.maxFrames ?? 12;
  let frames = 0;
  let stable = 0;
  const step = (): void => {
    const want = scroller.getBoundingClientRect().top + opts.stickyTop;
    const delta = target.getBoundingClientRect().top - want;
    if (Math.abs(delta) <= 1) {
      if (++stable >= 2) return;
    } else {
      stable = 0;
      scroller.scrollTop += delta;
    }
    if (++frames < maxFrames) raf(step);
  };
  raf(step);
}
