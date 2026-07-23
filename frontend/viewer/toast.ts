import { beginToastLeave, TOAST_LEAVE_MS, TOAST_LEAVING_ATTRIBUTE } from "../core/toast";

export { TOAST_LEAVE_MS, TOAST_LEAVING_ATTRIBUTE };
export const TOAST_ATTRIBUTE = "data-viewer-toast";
export const TOAST_DISMISS_DELAY_MS = 5000;

const installedDocuments = new WeakSet<Document>();
const dismissTimersByToast = new WeakMap<Element, ReturnType<typeof setTimeout>>();

// Each toast node gets one absolute timer: later swaps must neither reset nor
// stack it, so a server-rendered toast always starts leaving 5 s after it first
// appeared, then removes itself through the shared fade-out.
export function installToastDismiss(root: Document): void {
  if (installedDocuments.has(root)) return;
  installedDocuments.add(root);
  const dismissActiveToast = (): void => {
    const toast = root.querySelector(`[${TOAST_ATTRIBUTE}]`);
    if (!toast || dismissTimersByToast.has(toast)) return;
    const dismissTimer = setTimeout(() => {
      const removeTimer = beginToastLeave(toast, () => dismissTimersByToast.delete(toast));
      dismissTimersByToast.set(toast, removeTimer);
    }, TOAST_DISMISS_DELAY_MS);
    dismissTimersByToast.set(toast, dismissTimer);
  };
  root.addEventListener("htmx:afterSwap", dismissActiveToast);
  dismissActiveToast();
}
