/// <reference types="node" />
// The inline sources ship verbatim as <script> bodies. These tests pin the
// contract the copy step relies on: the committed generated snippets are
// byte-identical to their sources, and every source parses as plain
// JavaScript (a TS-only annotation would reach browsers as a syntax error).
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const SNIPPETS = [
  ["frontend/inline/theme-boot.ts", "crates/preview/src/embedded/generated/boot.js"],
  ["frontend/inline/theme-control.ts", "crates/desktop/src/embedded/generated/theme-control.js"],
  ["frontend/inline/pending-recipes.ts", "crates/desktop/src/embedded/generated/pending-recipes.js"],
] as const;

describe("server-inlined scripts", () => {
  for (const [source, generated] of SNIPPETS) {
    it(`${source} is valid plain JavaScript`, () => {
      const body = readFileSync(source, "utf8");
      expect(() => new Function(body)).not.toThrow();
      expect(body).not.toContain("\n");
    });

    it(`${generated} matches its source byte for byte`, () => {
      expect(readFileSync(generated, "utf8")).toBe(readFileSync(source, "utf8"));
    });
  }

  it("theme boot does not read or write layout and density preferences", () => {
    const body = readFileSync("frontend/inline/theme-boot.ts", "utf8");

    expect(body).not.toContain("gtl-diff-layout");
    expect(body).not.toContain("diffLayout");
    expect(body).not.toContain("diffFull");
  });

  it("theme control keeps presentation in server-rendered utilities", () => {
    const body = readFileSync("frontend/inline/theme-control.ts", "utf8");

    expect(body).toContain("target.dataset.viewerTheme");
    expect(body).toContain("document.documentElement.dataset.theme");
    expect(body).not.toContain("classList");
    expect(body).not.toContain("style.");
  });

  it("viewer toast feedback keeps one absolute timer per toast node", () => {
    const body = readFileSync("frontend/inline/theme-control.ts", "utf8");
    const listeners = new Map<string, EventListener>();
    const timers = new Map<number, () => void>();
    const timerDelays = new Map<number, number>();
    const clearedTimers: number[] = [];
    let timerIdNext = 1;
    let toastAttached = true;
    let toastRemovalCount = 0;
    const toast = {
      remove() {
        toastAttached = false;
        toastRemovalCount += 1;
      },
    };
    const documentFixture = {
      documentElement: { dataset: {} },
      querySelector: () => (toastAttached ? toast : null),
      addEventListener: (eventName: string, listener: EventListener) => {
        listeners.set(eventName, listener);
      },
    };
    const setTimeoutFixture = (callback: () => void, delayMilliseconds: number) => {
      const timerId = timerIdNext;
      timerIdNext += 1;
      timers.set(timerId, callback);
      timerDelays.set(timerId, delayMilliseconds);
      return timerId;
    };
    const clearTimeoutFixture = (timerId: number) => {
      timers.delete(timerId);
      clearedTimers.push(timerId);
    };

    new Function("document", "HTMLInputElement", "setTimeout", "clearTimeout", body)(
      documentFixture,
      class InputElementFixture {},
      setTimeoutFixture,
      clearTimeoutFixture,
    );
    const afterSwap = listeners.get("htmx:afterSwap");
    expect(afterSwap).toBeDefined();
    expect([...timerDelays.values()]).toEqual([5000]);
    expect([...timers.keys()]).toEqual([1]);

    afterSwap?.(new Event("htmx:afterSwap"));
    afterSwap?.(new Event("htmx:afterSwap"));

    expect([...timers.keys()]).toEqual([1]);
    expect(clearedTimers).toEqual([]);
    expect([...timerDelays.values()]).toEqual([5000]);

    const activeTimer = timers.get(1);
    timers.delete(1);
    activeTimer?.();

    expect(toastRemovalCount).toBe(1);
    expect(timers.size).toBe(0);

    toastAttached = true;
    afterSwap?.(new Event("htmx:afterSwap"));

    expect(clearedTimers).toEqual([]);
    expect([...timers.keys()]).toEqual([2]);
    expect(timerDelays.get(2)).toBe(5000);
  });
});
