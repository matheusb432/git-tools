import { installOnce } from "../shared/install-once";
import { computeWheelScroll } from "../shared/wheel";

/**
 * Vertical wheel over the tab strip scrolls it horizontally, so an overflowing
 * strip stays navigable with an ordinary mouse and not only the slim rail. The
 * listener is delegated on the document because the strip is replaced wholesale
 * by out-of-band swaps; a single non-passive listener survives every swap.
 */
export const installTabWheel = installOnce((root: Document): void => {
  root.addEventListener(
    "wheel",
    (event) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      const strip = target.closest<HTMLElement>(".viewer-tab-list");
      if (!strip) return;
      const next = computeWheelScroll(strip, event);
      if (next === null) return;
      event.preventDefault();
      strip.scrollLeft = next;
    },
    { passive: false },
  );
});
