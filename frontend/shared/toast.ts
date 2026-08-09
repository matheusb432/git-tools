// Framework-free transient toast: a bottom-center pill that fades in on insert
// (via @starting-style) and out via the data-leaving attribute. Shared by the
// client-created copy toast and the server-rendered swap toast's dismissal.

// One literal so Tailwind scans it; mirrors the swap toast in fragments/tabs.rs.
const TOAST_CLASSES =
  "gtl-toast pointer-events-none fixed bottom-6 left-1/2 z-50 flex max-w-[min(760px,calc(100vw-32px))] -translate-x-1/2 items-center gap-2 rounded-panel border border-acc-line bg-surface px-3.5 py-2 text-[12.5px] text-ink opacity-100 shadow-[0_6px_18px_rgba(0,0,0,.22)] transition-[opacity,scale] duration-200 ease-out [overflow-wrap:anywhere] starting:scale-95 starting:opacity-0 motion-reduce:transition-none before:font-bold before:text-add before:content-['\\2713'] [&[data-leaving]]:scale-95 [&[data-leaving]]:opacity-0 print:hidden!";

export const TOAST_LEAVING_ATTRIBUTE = "data-leaving";
/** Matches the toast's 200ms leave fade (the duration-200 utility). */
export const TOAST_LEAVE_MS = 200;
/** How long a copy toast stays fully shown before it begins leaving. */
export const TOAST_HOLD_MS = 1600;

let activeToast: HTMLElement | null = null;
// The hold and the leave are consecutive phases of one dismissal, never concurrent.
let dismissTimer: ReturnType<typeof setTimeout> | null = null;

/** Fades `toast` out, then removes it and runs `onRemoved`. Returns the removal timer. */
export function beginToastLeave(toast: Element, onRemoved: () => void = () => {}): ReturnType<typeof setTimeout> {
  toast.setAttribute(TOAST_LEAVING_ATTRIBUTE, "");
  return setTimeout(() => {
    toast.remove();
    onRemoved();
  }, TOAST_LEAVE_MS);
}

/**
 * Shows a self-dismissing toast pill. A fresh element is inserted on each call so
 * the enter animation replays, replacing any toast still on screen at once.
 */
export function showToast(message: string, container: ParentNode = document.body): void {
  if (dismissTimer !== null) clearTimeout(dismissTimer);
  activeToast?.remove();

  const el = document.createElement("div");
  el.className = TOAST_CLASSES;
  el.setAttribute("role", "status");
  el.setAttribute("aria-live", "polite");
  el.setAttribute("aria-atomic", "true");
  el.textContent = message;
  container.appendChild(el);
  activeToast = el;
  dismissTimer = setTimeout(() => {
    dismissTimer = beginToastLeave(el, () => {
      dismissTimer = null;
      activeToast = null;
    });
  }, TOAST_HOLD_MS);
}
