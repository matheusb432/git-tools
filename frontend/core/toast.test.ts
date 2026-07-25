import { describe, expect, test, vi } from "vitest";
import { useFakeToastTimers } from "../test/dom-stub";
import { showToast, TOAST_HOLD_MS, TOAST_LEAVE_MS, TOAST_LEAVING_ATTRIBUTE } from "./toast";

describe("showToast", () => {
  useFakeToastTimers();

  test("inserts one announced pill carrying the message", () => {
    showToast("Copied with context");

    const toast = document.querySelector(".gtl-toast");
    expect(toast?.textContent).toBe("Copied with context");
    expect(toast?.getAttribute("role")).toBe("status");
    expect(toast?.getAttribute("aria-live")).toBe("polite");
    expect(toast?.getAttribute("aria-atomic")).toBe("true");
  });

  test("a later toast replaces the earlier one at once", () => {
    showToast("first");
    showToast("second");

    const toasts = document.querySelectorAll(".gtl-toast");
    expect(toasts.length).toBe(1);
    expect(toasts[0]?.textContent).toBe("second");
  });

  test("fades out after the hold, then removes itself", () => {
    showToast("done");
    const toast = document.querySelector(".gtl-toast");

    vi.advanceTimersByTime(TOAST_HOLD_MS);
    expect(toast?.getAttribute(TOAST_LEAVING_ATTRIBUTE)).toBe("");
    expect(toast?.parentElement).not.toBeNull();

    vi.advanceTimersByTime(TOAST_LEAVE_MS);
    expect(document.querySelector(".gtl-toast")).toBeNull();
  });
});
