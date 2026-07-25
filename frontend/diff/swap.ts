import { installOnce } from "../shared/install-once";
import { scrollLandOn } from "./scroll";
import { enhanceControls } from "./controls";
import { enhanceLayout } from "./enhance-layout";

export type EnhancementLifecycle = {
  readonly enhanceWithin: (scope: ParentNode) => void;
  readonly destroyWithin: (scope: ParentNode) => void;
};

/** Where the view was reading before a swap: a file plus its viewport offset, or bare scroll. */
export type SwapAnchor =
  | { readonly path: string; readonly offset: number; readonly scrollTop: number }
  | { readonly path: null; readonly scrollTop: number };

type ResponseHeaderReader = { readonly getResponseHeader: (name: string) => string | null };

type LandOnAnchor = (target: HTMLDetailsElement, scroller: HTMLElement, offset: number) => void;

const pendingAnchors = new WeakMap<HTMLElement, SwapAnchor>();
const RECOVERY_HEADER = "X-GTL-Recovery";

function layoutsWithin(scope: ParentNode): readonly HTMLElement[] {
  if (scope instanceof HTMLElement && scope.matches(".layout")) return [scope];
  return [...scope.querySelectorAll<HTMLElement>(".layout")];
}

export function createEnhancementLifecycle(mountRoot: (root: HTMLElement) => () => void): EnhancementLifecycle {
  const mounted = new WeakMap<HTMLElement, () => void>();
  return {
    enhanceWithin(scope) {
      layoutsWithin(scope).forEach((root) => {
        if (mounted.has(root)) return;
        mounted.set(root, mountRoot(root));
        root.dataset["gtlEnhanced"] = "true";
      });
    },
    destroyWithin(scope) {
      layoutsWithin(scope).forEach((root) => {
        const unmount = mounted.get(root);
        if (!unmount) return;
        unmount();
        mounted.delete(root);
        delete root.dataset["gtlEnhanced"];
      });
    },
  };
}

const enhancementLifecycle = createEnhancementLifecycle((root) => {
  const cleanups = [enhanceControls(root), enhanceLayout(root)];
  return () => {
    cleanups.reverse().forEach((cleanup) => cleanup());
  };
});

export const enhanceWithin = enhancementLifecycle.enhanceWithin;
export const destroyWithin = enhancementLifecycle.destroyWithin;

export function captureSwapAnchor(view: ParentNode): SwapAnchor {
  const scroller = view.querySelector<HTMLElement>(".main");
  if (!scroller) return { path: null, scrollTop: 0 };
  const scrollerTop = scroller.getBoundingClientRect().top;
  // One forced layout per candidate is the cost here, so each file's rect is read once.
  for (const file of view.querySelectorAll<HTMLDetailsElement>("details.file[data-path]")) {
    if (file.hidden) continue;
    const rect = file.getBoundingClientRect();
    if (rect.bottom <= scrollerTop) continue;
    const path = file.getAttribute("data-path");
    if (path === null) continue;
    return { path, offset: rect.top - scrollerTop, scrollTop: scroller.scrollTop };
  }
  return { path: null, scrollTop: scroller.scrollTop };
}

export function restoreSwapAnchor(
  view: ParentNode,
  anchor: SwapAnchor,
  land: LandOnAnchor = (target, scroller, offset) => scrollLandOn(target, scroller, { stickyTop: offset }),
): void {
  const scroller = view.querySelector<HTMLElement>(".main");
  if (!scroller) return;
  if (anchor.path !== null) {
    const target = [...view.querySelectorAll<HTMLDetailsElement>("details.file[data-path]")].find(
      (file) => file.getAttribute("data-path") === anchor.path,
    );
    if (target) {
      target.open = true;
      land(target, scroller, anchor.offset);
      return;
    }
  }
  scroller.scrollTop = Math.min(
    Math.max(anchor.scrollTop, 0),
    Math.max(scroller.scrollHeight - scroller.clientHeight, 0),
  );
}

function eventTarget(event: Event): HTMLElement | null {
  if (!("detail" in event)) return null;
  const detail = event.detail;
  if (typeof detail !== "object" || detail === null || !("target" in detail)) return null;
  return detail.target instanceof HTMLElement ? detail.target : null;
}

function swapWillRun(event: Event): boolean {
  if (!("detail" in event)) return false;
  const detail = event.detail;
  return typeof detail === "object" && detail !== null && (!("shouldSwap" in detail) || detail.shouldSwap !== false);
}

function readsResponseHeaders(value: unknown): value is ResponseHeaderReader {
  return (
    typeof value === "object" &&
    value !== null &&
    "getResponseHeader" in value &&
    typeof value.getResponseHeader === "function"
  );
}

function optIntoMarkedRecoverySwap(event: Event): void {
  if (!("detail" in event)) return;
  const detail = event.detail;
  if (
    typeof detail !== "object" ||
    detail === null ||
    !("xhr" in detail) ||
    !("shouldSwap" in detail) ||
    !("isError" in detail)
  ) {
    return;
  }
  const xhr = detail.xhr;
  if (!readsResponseHeaders(xhr) || xhr.getResponseHeader(RECOVERY_HEADER) !== "true") return;
  detail.shouldSwap = true;
  detail.isError = false;
}

function replacementTarget(event: Event): HTMLElement | null {
  return event.target instanceof HTMLElement ? event.target : null;
}

function isViewerView(target: HTMLElement): boolean {
  return target.id === "viewer-view";
}

export const installSwapLifecycle = installOnce(
  (targetDocument: Document, lifecycle: EnhancementLifecycle = enhancementLifecycle): void => {
    const beforeSwap = (event: Event): void => {
      optIntoMarkedRecoverySwap(event);
      const target = eventTarget(event);
      if (!target || !isViewerView(target) || !swapWillRun(event)) return;
      if (!pendingAnchors.has(target)) pendingAnchors.set(target, captureSwapAnchor(target));
      lifecycle.destroyWithin(target);
    };
    const afterSwap = (event: Event): void => {
      const oldTarget = eventTarget(event);
      const replacement = replacementTarget(event);
      if (!oldTarget || !isViewerView(oldTarget) || !replacement || !isViewerView(replacement)) return;
      lifecycle.enhanceWithin(replacement);
      const anchor = pendingAnchors.get(oldTarget);
      if (anchor) {
        restoreSwapAnchor(replacement, anchor);
        pendingAnchors.delete(oldTarget);
      }
    };
    targetDocument.addEventListener("htmx:beforeSwap", beforeSwap);
    targetDocument.addEventListener("htmx:afterSwap", afterSwap);
    targetDocument.addEventListener("htmx:oobBeforeSwap", beforeSwap);
    targetDocument.addEventListener("htmx:oobAfterSwap", afterSwap);
  },
);
