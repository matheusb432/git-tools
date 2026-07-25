import { describe, expect, test } from "vitest";
import { installTabWheel } from "./tab-scroll";

function tabStrip(): HTMLElement {
  const strip = document.createElement("div");
  strip.className = "viewer-tab-list";
  strip.scrollLeft = 0;
  Object.defineProperties(strip, {
    scrollWidth: { configurable: true, value: 400 },
    clientWidth: { configurable: true, value: 100 },
  });
  document.body.appendChild(strip);
  return strip;
}

function wheelOver(strip: HTMLElement, deltaY: number): Event {
  const event = new Event("wheel", { bubbles: true, cancelable: true });
  Object.defineProperties(event, {
    deltaX: { value: 0 },
    deltaY: { value: deltaY },
    ctrlKey: { value: false },
  });
  strip.dispatchEvent(event);
  return event;
}

describe("installTabWheel", () => {
  test("a vertical wheel over the strip scrolls it horizontally and cancels the page scroll", () => {
    const strip = tabStrip();
    installTabWheel(document);

    const event = wheelOver(strip, 40);

    expect(strip.scrollLeft).toBe(40);
    expect(event.defaultPrevented).toBe(true);
  });

  test("the delegated listener survives a wholesale strip replacement and wires the document once", () => {
    installTabWheel(document);
    tabStrip();
    document.body.replaceChildren();
    const replacement = tabStrip();
    installTabWheel(document);

    wheelOver(replacement, 40);

    expect(replacement.scrollLeft).toBe(40);
  });
});
