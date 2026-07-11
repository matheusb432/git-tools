import { mount, unmount } from "svelte";
import { scrollLandOn } from "../core/scroll";
import App from "./App.svelte";

type MountedApp = ReturnType<typeof mount>;

export type EnhancementLifecycle = {
  readonly enhanceWithin: (scope: ParentNode) => void;
  readonly destroyWithin: (scope: ParentNode) => void;
};

export type SwapAnchor = {
  readonly path: string | null;
  readonly offset: number;
  readonly scrollTop: number;
};

type LandOnAnchor = (target: HTMLDetailsElement, scroller: HTMLElement, offset: number) => void;

const installedDocuments = new WeakSet<Document>();
const pendingAnchors = new WeakMap<HTMLElement, SwapAnchor>();

function layoutsWithin(scope: ParentNode): readonly HTMLElement[] {
  if (scope instanceof HTMLElement && scope.matches(".layout")) return [scope];
  return [...scope.querySelectorAll<HTMLElement>(".layout")];
}

export function createEnhancementLifecycle<Handle extends object>(
  mountRoot: (root: HTMLElement) => Handle,
  unmountRoot: (handle: Handle) => void,
): EnhancementLifecycle {
  const mounted = new WeakMap<HTMLElement, Handle>();
  return {
    enhanceWithin(scope) {
      layoutsWithin(scope).forEach((root) => {
        if (mounted.has(root)) return;
        const component = mountRoot(root);
        mounted.set(root, component);
        root.dataset["gtlEnhanced"] = "true";
      });
    },
    destroyWithin(scope) {
      layoutsWithin(scope).forEach((root) => {
        const component = mounted.get(root);
        if (!component) return;
        unmountRoot(component);
        mounted.delete(root);
        delete root.dataset["gtlEnhanced"];
      });
    },
  };
}

const enhancementLifecycle = createEnhancementLifecycle<MountedApp>(
  (root) => mount(App, { target: root, props: { root } }),
  (component) => {
    void unmount(component);
  },
);

export const enhanceWithin = enhancementLifecycle.enhanceWithin;
export const destroyWithin = enhancementLifecycle.destroyWithin;

export function captureSwapAnchor(view: ParentNode): SwapAnchor {
  const scroller = view.querySelector<HTMLElement>(".main");
  if (!scroller) return { path: null, offset: 0, scrollTop: 0 };
  const scrollerTop = scroller.getBoundingClientRect().top;
  const topFile = [...view.querySelectorAll<HTMLDetailsElement>("details.file[data-path]")].find(
    (file) => !file.hidden && file.getBoundingClientRect().bottom > scrollerTop,
  );
  return {
    path: topFile?.getAttribute("data-path") ?? null,
    offset: topFile ? topFile.getBoundingClientRect().top - scrollerTop : 0,
    scrollTop: scroller.scrollTop,
  };
}

export function restoreSwapAnchor(
  view: ParentNode,
  anchor: SwapAnchor,
  land: LandOnAnchor = (target, scroller, offset) => scrollLandOn(target, scroller, { stickyTop: offset }),
): void {
  const scroller = view.querySelector<HTMLElement>(".main");
  if (!scroller) return;
  const target = anchor.path
    ? [...view.querySelectorAll<HTMLDetailsElement>("details.file[data-path]")].find(
        (file) => file.getAttribute("data-path") === anchor.path,
      )
    : undefined;
  if (target) {
    target.open = true;
    land(target, scroller, anchor.offset);
    return;
  }
  scroller.scrollTop = Math.min(Math.max(anchor.scrollTop, 0), Math.max(scroller.scrollHeight - scroller.clientHeight, 0));
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

function replacementTarget(event: Event): HTMLElement | null {
  return event.target instanceof HTMLElement ? event.target : null;
}

function isViewerView(target: HTMLElement): boolean {
  return target.id === "viewer-view";
}

export function installSwapLifecycle(
  targetDocument: Document,
  lifecycle: EnhancementLifecycle = enhancementLifecycle,
): void {
  if (installedDocuments.has(targetDocument)) return;
  installedDocuments.add(targetDocument);
  const beforeSwap = (event: Event): void => {
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
}
