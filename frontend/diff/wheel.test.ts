import "../test/dom-stub"; // ensure globalThis.document exists before any imports
import { expect, test, describe } from "bun:test";
import { computeWheelScroll } from "./wheel";
import { isShaTarget } from "./commit-focus";
import { buildFileLeaf } from "./file-tree";

// ---------------------------------------------------------------------------
// Gap 1: computeWheelScroll
// ---------------------------------------------------------------------------

describe("computeWheelScroll", () => {
  function scroller(scrollLeft = 0, scrollWidth = 200, clientWidth = 100) {
    return { scrollLeft, scrollWidth, clientWidth };
  }
  function ev(deltaY = 0, deltaX = 0, ctrlKey = false) {
    return { deltaY, deltaX, ctrlKey };
  }

  test("bails when ctrlKey is set (pinch-zoom)", () => {
    expect(computeWheelScroll(scroller(), ev(10, 0, true))).toBeNull();
  });

  test("bails when there is no overflow (max <= 0)", () => {
    // clientWidth >= scrollWidth → no horizontal room
    expect(computeWheelScroll(scroller(0, 100, 100), ev(10))).toBeNull();
    expect(computeWheelScroll(scroller(0, 80, 100), ev(10))).toBeNull();
  });

  test("bails when both deltas are 0", () => {
    expect(computeWheelScroll(scroller(), ev(0, 0))).toBeNull();
  });

  test("uses deltaY when |deltaY| >= |deltaX|", () => {
    // deltaY = 20, deltaX = 5: dominant axis is Y
    expect(computeWheelScroll(scroller(0), ev(20, 5))).toBe(20);
  });

  test("uses deltaX when |deltaX| > |deltaY|", () => {
    // deltaY = 3, deltaX = 15: dominant axis is X
    expect(computeWheelScroll(scroller(0), ev(3, 15))).toBe(15);
  });

  test("clamps to 0 (scroll past left edge)", () => {
    // scrollLeft = 5, delta = -30 → next would be -25, clamped to 0
    expect(computeWheelScroll(scroller(5), ev(-30))).toBe(0);
  });

  test("clamps to max (scroll past right edge)", () => {
    // max = 100, scrollLeft = 90, delta = 30 → next would be 120, clamped to 100
    expect(computeWheelScroll(scroller(90), ev(30))).toBe(100);
  });

  test("bails when scrollLeft is already at the computed target", () => {
    // scrollLeft = 100 (== max), delta = 50 → clamped next = 100 = scrollLeft → bail
    expect(computeWheelScroll(scroller(100), ev(50))).toBeNull();
  });

  test("returns exact mid-range value", () => {
    expect(computeWheelScroll(scroller(30), ev(20))).toBe(50);
  });
});

// ---------------------------------------------------------------------------
// Gap 2: isShaTarget
// ---------------------------------------------------------------------------

describe("isShaTarget", () => {
  test("returns true when target.closest('.sha') is truthy", () => {
    const target = { closest: (s: string) => (s === ".sha" ? {} : null) };
    expect(isShaTarget(target)).toBe(true);
  });

  test("returns false when target.closest('.sha') returns null", () => {
    const target = { closest: (_s: string) => null };
    expect(isShaTarget(target)).toBe(false);
  });

  test("returns false when target is null", () => {
    expect(isShaTarget(null)).toBe(false);
  });

  test("returns false when target has no closest method", () => {
    expect(isShaTarget({})).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Gap 3: buildFileLeaf — DOM order and class contract
// ---------------------------------------------------------------------------

describe("buildFileLeaf", () => {
  // Minimal Document stub: createElement records calls and returns real-ish objects.
  function makeDoc() {
    function createElement(tag: string): HTMLElement {
      // Build a lightweight node that tracks children in order.
      const el: Record<string, unknown> & {
        tag: string;
        className: string;
        textContent: string;
        title: string;
        children: HTMLElement[];
        attrs: Record<string, string>;
        appendChild(c: HTMLElement): void;
        setAttribute(k: string, v: string): void;
        getAttribute(k: string): string | null;
        querySelector(sel: string): HTMLElement | null;
      } = {
        tag,
        className: "",
        textContent: "",
        title: "",
        children: [],
        attrs: {},
        appendChild(c: HTMLElement) {
          this.children.push(c);
        },
        setAttribute(k: string, v: string) {
          this.attrs[k] = v;
        },
        getAttribute(k: string) {
          return this.attrs[k] ?? null;
        },
        querySelector(sel: string): HTMLElement | null {
          // depth-first search through children
          for (const child of this.children) {
            const c = child as unknown as typeof el;
            if (sel === `.tlabel` && c.className === "tlabel") return child;
            const found = c.querySelector(sel);
            if (found) return found;
          }
          return null;
        },
      };
      return el as unknown as HTMLElement;
    }
    return { createElement } as unknown as Document;
  }

  const file = {
    name: "main.ts",
    status: "added",
    statusCode: "A",
    statusLabel: "Added file",
    el: { id: "file-main-ts" },
  };

  test("li has class tfile and status-<status>", () => {
    const doc = makeDoc();
    const li = buildFileLeaf(doc, file) as unknown as { className: string };
    expect(li.className).toContain("tfile");
    expect(li.className).toContain("status-added");
  });

  test("tlabel children are [name, status] in that order", () => {
    const doc = makeDoc();
    const li = buildFileLeaf(doc, file) as unknown as {
      children: Array<{ className: string; children: Array<{ className: string }> }>;
    };
    const label = li.children[0]!; // first child is .tlabel
    expect(label.className).toBe("tlabel");
    expect(label.children.length).toBe(2);
    expect(label.children[0]!.className).toContain("tname"); // name first
    expect(label.children[1]!.className).toContain("tstatus"); // status trailing
  });

  test("status span textContent is the statusCode", () => {
    const doc = makeDoc();
    const li = buildFileLeaf(doc, file) as unknown as {
      children: Array<{ children: Array<{ textContent: string }> }>;
    };
    const label = li.children[0]!;
    const statusSpan = label.children[1]!;
    expect(statusSpan.textContent).toBe("A");
  });

  test("no child has a class containing 'icon'", () => {
    const doc = makeDoc();
    const li = buildFileLeaf(doc, file) as unknown as {
      children: Array<{ className: string; children: Array<{ className: string }> }>;
    };
    function hasIconClass(node: { className: string; children?: Array<unknown> }): boolean {
      if (node.className.includes("icon")) return true;
      for (const c of node.children ?? []) {
        if (hasIconClass(c as { className: string; children: Array<unknown> })) return true;
      }
      return false;
    }
    expect(hasIconClass(li as unknown as { className: string; children: Array<unknown> })).toBe(false);
  });

  test("data-target is set to el.id", () => {
    const doc = makeDoc();
    const li = buildFileLeaf(doc, file) as unknown as { attrs: Record<string, string> };
    expect(li.attrs["data-target"]).toBe("file-main-ts");
  });
});
