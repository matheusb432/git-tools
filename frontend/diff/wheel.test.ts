import { describe, expect, test } from "vitest";
import { isShaTarget } from "./commit-focus";
import { buildFileLeaf } from "./file-tree";
import { computeWheelScroll } from "../core/wheel";

describe("computeWheelScroll", () => {
  function scroller(scrollLeft = 0, scrollWidth = 200, clientWidth = 100) {
    return { scrollLeft, scrollWidth, clientWidth };
  }
  function ev(deltaY = 0, deltaX = 0, ctrlKey = false) {
    return { deltaY, deltaX, ctrlKey };
  }

  test("bails on ctrl (pinch-zoom)", () => {
    expect(computeWheelScroll(scroller(), ev(10, 0, true))).toBeNull();
  });

  test("bails without horizontal overflow", () => {
    expect(computeWheelScroll(scroller(0, 100, 100), ev(10))).toBeNull();
    expect(computeWheelScroll(scroller(0, 80, 100), ev(10))).toBeNull();
  });

  test("bails when both deltas are zero", () => {
    expect(computeWheelScroll(scroller(), ev(0, 0))).toBeNull();
  });

  test("follows the dominant axis", () => {
    expect(computeWheelScroll(scroller(0), ev(20, 5))).toBe(20);
    expect(computeWheelScroll(scroller(0), ev(3, 15))).toBe(15);
  });

  test("clamps to both edges", () => {
    expect(computeWheelScroll(scroller(5), ev(-30))).toBe(0);
    expect(computeWheelScroll(scroller(90), ev(30))).toBe(100);
  });

  test("bails when already at the clamped target", () => {
    expect(computeWheelScroll(scroller(100), ev(50))).toBeNull();
  });

  test("scrolls by the delta mid-range", () => {
    expect(computeWheelScroll(scroller(30), ev(20))).toBe(50);
  });
});

describe("isShaTarget", () => {
  test("true when the target resolves the .sha button", () => {
    const target = { closest: (selector: string) => (selector === ".sha" ? {} : null) };
    expect(isShaTarget(target)).toBe(true);
  });

  test("false outside the button, without closest, or without a target", () => {
    expect(isShaTarget({ closest: () => null })).toBe(false);
    expect(isShaTarget({})).toBe(false);
    expect(isShaTarget(null)).toBe(false);
  });
});

describe("buildFileLeaf", () => {
  const file = {
    name: "main.ts",
    status: "added",
    statusCode: "A",
    statusLabel: "Added file",
    el: { id: "file-main-ts" },
  };

  test("builds a status-classed leaf targeting its file element", () => {
    const li = buildFileLeaf(document, file);

    expect(li.className).toContain("tfile");
    expect(li.className).toContain("status-added");
    expect(li.getAttribute("data-target")).toBe("file-main-ts");
  });

  test("labels the leaf with the name first and the status badge last", () => {
    const li = buildFileLeaf(document, file);

    const parts = [...li.querySelectorAll(".tname, .tstatus")];
    expect(parts.map((part) => part.textContent)).toEqual(["main.ts", "A"]);
    expect(parts[1]?.getAttribute("aria-label")).toBe("Added file");
  });
});
