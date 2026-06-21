// ! Land scroll exactly on `target`: content-visibility boxes realize their true height
// ! mid-scroll, shifting the target — so re-measure each frame and re-align until stable.
export function scrollLandOn(
  target: HTMLElement,
  scroller: HTMLElement,
  opts: { stickyTop: number; raf?: (cb: FrameRequestCallback) => number; maxFrames?: number },
): void {
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
