import { describe, expect, test, vi } from "vitest";
import { TOAST_LEAVE_MS, TOAST_LEAVING_ATTRIBUTE } from "../shared/toast";
import { useFakeToastTimers } from "../test/dom-stub";
import { installToastDismiss, TOAST_ATTRIBUTE, TOAST_DISMISS_DELAY_MS } from "./toast";

// installToastDismiss wires one document at most once, so the first test also
// owns the install-time dismissal path; later tests go through htmx:afterSwap.
describe("installToastDismiss", () => {
  useFakeToastTimers();

  function appendToast(): HTMLElement {
    const toast = document.createElement("div");
    toast.setAttribute(TOAST_ATTRIBUTE, "");
    document.body.appendChild(toast);
    return toast;
  }

  test("later swaps neither reset nor stack the toast's absolute timer", () => {
    const toast = appendToast();
    installToastDismiss(document);
    document.dispatchEvent(new Event("htmx:afterSwap"));

    vi.advanceTimersByTime(TOAST_DISMISS_DELAY_MS - 1000);
    document.dispatchEvent(new Event("htmx:afterSwap"));
    vi.advanceTimersByTime(999);
    expect(toast.getAttribute(TOAST_LEAVING_ATTRIBUTE)).toBeNull();

    vi.advanceTimersByTime(1);
    expect(toast.getAttribute(TOAST_LEAVING_ATTRIBUTE)).toBe("");
    document.dispatchEvent(new Event("htmx:afterSwap"));

    vi.advanceTimersByTime(TOAST_LEAVE_MS);
    expect(toast.parentElement).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });

  test("a toast swapped in after install gets its own full delay", () => {
    installToastDismiss(document);
    const toast = appendToast();
    document.dispatchEvent(new Event("htmx:afterSwap"));

    vi.advanceTimersByTime(TOAST_DISMISS_DELAY_MS);

    expect(toast.getAttribute(TOAST_LEAVING_ATTRIBUTE)).toBe("");
  });
});
