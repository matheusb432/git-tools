import { describe, expect, test } from "vitest";
import { captureDiffDocumentAnchor, restoreDiffDocumentAnchor } from "./enhance-document";

function must<T>(value: T | null, what: string): T {
  if (value === null) throw new Error(`${what} is missing`);
  return value;
}

function documentWithFile(path: string): HTMLElement {
  const root = document.createElement("section");
  const scroller = document.createElement("main");
  scroller.className = "main";
  const file = document.createElement("details");
  file.className = "file";
  file.setAttribute("data-path", path);
  scroller.appendChild(file);
  root.appendChild(scroller);
  return root;
}

describe("diff document anchor", () => {
  test("restores the top visible file, opens it, and preserves its viewport offset", () => {
    const before = documentWithFile("src/lib.rs");
    const beforeScroller = must(before.querySelector<HTMLElement>(".main"), "the anchor scroller");
    const beforeFile = must(before.querySelector<HTMLDetailsElement>("details.file"), "the anchor file");
    beforeScroller.scrollTop = 83;
    setTestRect(beforeScroller, { y: 40, height: 200 });
    setTestRect(beforeFile, { y: 54, height: 80 });

    const snapshot = captureDiffDocumentAnchor(before);
    const next = documentWithFile("src/lib.rs");
    let restoredOffset: number | undefined;
    restoreDiffDocumentAnchor(next, snapshot, (_target, _scroller, offset) => {
      restoredOffset = offset;
    });

    expect(restoredOffset).toBe(14);
    expect(next.querySelector<HTMLDetailsElement>("details.file")?.open).toBe(true);
  });

  test("falls back to the clamped numeric scroll position when the anchor file disappeared", () => {
    const before = documentWithFile("src/old.rs");
    const beforeScroller = must(before.querySelector<HTMLElement>(".main"), "the anchor scroller");
    beforeScroller.scrollTop = 240;
    const snapshot = captureDiffDocumentAnchor(before);

    const next = documentWithFile("src/new.rs");
    const nextScroller = must(next.querySelector<HTMLElement>(".main"), "the replacement scroller");
    Object.defineProperties(nextScroller, {
      scrollHeight: { configurable: true, value: 150 },
      clientHeight: { configurable: true, value: 100 },
    });
    restoreDiffDocumentAnchor(next, snapshot);

    expect(nextScroller.scrollTop).toBe(50);
  });
});
