import { installOnce } from "../shared/install-once";
import { enhanceLayout } from "./enhance-layout";
import { captureDiffDocumentAnchor, restoreDiffDocumentAnchor, type DiffDocumentAnchor } from "./enhance-document";
import { createEnhancementLifecycle, type EnhancementLifecycle } from "./enhancement-lifecycle";

type ResponseHeaderReader = { readonly getResponseHeader: (name: string) => string | null };

type PendingSwap = {
  readonly anchor: DiffDocumentAnchor;
  readonly viewIdentity: string | null;
};

const pendingSwaps = new WeakMap<HTMLElement, PendingSwap>();
const RECOVERY_HEADER = "X-GTL-Recovery";

const enhancementLifecycle = createEnhancementLifecycle(".layout", enhanceLayout);

export const enhanceWithin = enhancementLifecycle.enhanceWithin;
export const destroyWithin = enhancementLifecycle.destroyWithin;

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
      if (!pendingSwaps.has(target)) {
        pendingSwaps.set(target, {
          anchor: captureDiffDocumentAnchor(target),
          viewIdentity: target.getAttribute("data-view-identity"),
        });
      }
      lifecycle.destroyWithin(target);
    };
    const afterSwap = (event: Event): void => {
      const oldTarget = eventTarget(event);
      const replacement = replacementTarget(event);
      if (!oldTarget || !isViewerView(oldTarget) || !replacement || !isViewerView(replacement)) return;
      lifecycle.enhanceWithin(replacement);
      const pending = pendingSwaps.get(oldTarget);
      if (pending) {
        if (pending.viewIdentity === replacement.getAttribute("data-view-identity")) {
          restoreDiffDocumentAnchor(replacement, pending.anchor);
        }
        pendingSwaps.delete(oldTarget);
      }
    };
    targetDocument.addEventListener("htmx:beforeSwap", beforeSwap);
    targetDocument.addEventListener("htmx:afterSwap", afterSwap);
    targetDocument.addEventListener("htmx:oobBeforeSwap", beforeSwap);
    targetDocument.addEventListener("htmx:oobAfterSwap", afterSwap);
  },
);

export { captureDiffDocumentAnchor as captureSwapAnchor, restoreDiffDocumentAnchor as restoreSwapAnchor };
export { createEnhancementLifecycle } from "./enhancement-lifecycle";
export type { EnhancementLifecycle } from "./enhancement-lifecycle";
