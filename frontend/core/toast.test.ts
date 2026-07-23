import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { beginToastLeave, showToast, TOAST_HOLD_MS, TOAST_LEAVE_MS, TOAST_LEAVING_ATTRIBUTE } from "./toast";

describe("showToast", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  test("inserts one announced pill carrying the message", () => {
    showToast("Copied with context");

    const toast = document.querySelector(".gtl-toast");
    expect(toast?.textContent).toBe("Copied with context");
    expect(toast?.getAttribute("role")).toBe("status");
    expect(toast?.getAttribute("aria-live")).toBe("polite");
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

describe("beginToastLeave", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  test("marks the node leaving, then removes it and reports removal", () => {
    const el = document.createElement("div");
    document.body.appendChild(el);
    let removed = false;

    beginToastLeave(el, () => {
      removed = true;
    });
    expect(el.getAttribute(TOAST_LEAVING_ATTRIBUTE)).toBe("");
    expect(el.parentElement).not.toBeNull();

    vi.advanceTimersByTime(TOAST_LEAVE_MS);
    expect(el.parentElement).toBeNull();
    expect(removed).toBe(true);
  });
});
