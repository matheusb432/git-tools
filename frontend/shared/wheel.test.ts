import { describe, expect, test } from "vitest";
import { computeWheelScroll } from "./wheel";

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
});
