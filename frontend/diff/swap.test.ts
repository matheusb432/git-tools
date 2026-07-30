import { describe, expect, test } from "vitest";
import { enhanceControls } from "./controls";
import { enhanceLayout } from "./enhance-layout";
import { captureSwapAnchor, createEnhancementLifecycle, installSwapLifecycle, restoreSwapAnchor } from "./swap";

function must<T>(value: T | null, what: string): T {
  if (value === null) throw new Error(`${what} is missing`);
  return value;
}

function layout(name: string): HTMLElement {
  const root = document.createElement("div");
  root.className = "layout";
  root.dataset["name"] = name;
  const fold = document.createElement("button");
  fold.className = "foldall";
  const file = document.createElement("details");
  file.className = "file";
  file.setAttribute("data-path", `${name}.rs`);
  const main = document.createElement("main");
  main.className = "main";
  main.appendChild(file);
  root.appendChild(fold);
  root.appendChild(main);
  return root;
}

function viewWithFile(path: string, identity?: string): HTMLElement {
  const view = document.createElement("section");
  view.id = "viewer-view";
  if (identity) view.setAttribute("data-view-identity", identity);
  const root = layout("view");
  must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file").setAttribute("data-path", path);
  view.appendChild(root);
  return view;
}

type SwapEventName = "htmx:beforeSwap" | "htmx:afterSwap" | "htmx:oobBeforeSwap" | "htmx:oobAfterSwap";

type SwapDetail = {
  readonly target: HTMLElement;
  shouldSwap: boolean;
  isError: boolean;
  readonly xhr?: { readonly getResponseHeader: (name: string) => string | null };
};

function dispatchSwap(
  type: SwapEventName,
  origin: HTMLElement,
  detailTarget: HTMLElement,
  shouldSwap = true,
  marker: string | null = null,
): SwapDetail {
  const event = new Event(type, { bubbles: true });
  const detail: SwapDetail = {
    target: detailTarget,
    shouldSwap,
    isError: !shouldSwap,
    xhr: { getResponseHeader: (name) => (name === "X-GTL-Recovery" ? marker : null) },
  };
  Object.defineProperty(event, "detail", { value: detail });
  origin.dispatchEvent(event);
  return detail;
}

// Mirrors swap.ts's production mount so the lifecycle covers everything a real swap wires.
const testLifecycle = createEnhancementLifecycle((root) => {
  const cleanups = [enhanceControls(root), enhanceLayout(root)];
  return () => {
    cleanups.reverse().forEach((cleanup) => cleanup());
  };
});

test("enhances and tears down each layout independently", () => {
  const first = layout("first");
  document.body.appendChild(first);
  // A second mount would double-bind .foldall, so the fold click below would toggle twice.
  testLifecycle.enhanceWithin(first);
  testLifecycle.enhanceWithin(first);
  expect(first.dataset["gtlEnhanced"]).toBe("true");

  const firstFile = must(first.querySelector<HTMLDetailsElement>("details.file"), "the first file");
  const firstFold = must(first.querySelector<HTMLButtonElement>(".foldall"), "the first fold button");
  firstFold.click();
  expect(firstFile.open).toBe(true);
  testLifecycle.destroyWithin(first);
  firstFold.click();
  expect(firstFile.open).toBe(true);
  expect(first.dataset["gtlEnhanced"]).toBeUndefined();

  installSwapLifecycle(document, testLifecycle);
  const requestTarget = document.createElement("section");
  requestTarget.id = "viewer-view";
  const replacementView = document.createElement("section");
  replacementView.id = "viewer-view";
  const replacement = layout("replacement");
  replacementView.appendChild(replacement);
  document.body.appendChild(replacementView);
  dispatchSwap("htmx:afterSwap", replacementView, requestTarget);
  expect(replacement.dataset["gtlEnhanced"]).toBe("true");
  const replacementFile = must(replacement.querySelector<HTMLDetailsElement>("details.file"), "the swapped file");
  const replacementFold = must(replacement.querySelector<HTMLButtonElement>(".foldall"), "the swapped fold button");
  replacementFold.click();
  expect(replacementFile.open).toBe(true);

  dispatchSwap("htmx:beforeSwap", replacementView, replacementView);
  const secondView = document.createElement("section");
  secondView.id = "viewer-view";
  const second = layout("second");
  secondView.appendChild(second);
  replacementView.remove();
  document.body.appendChild(secondView);
  dispatchSwap("htmx:afterSwap", secondView, replacementView);
  const secondFile = must(second.querySelector<HTMLDetailsElement>("details.file"), "the second file");
  const secondFold = must(second.querySelector<HTMLButtonElement>(".foldall"), "the second fold button");
  secondFold.click();
  expect(secondFile.open).toBe(true);
});

describe("swap anchor", () => {
  test("restores the top visible file, opens it, and preserves its viewport offset", () => {
    const before = viewWithFile("src/lib.rs");
    const beforeScroller = must(before.querySelector<HTMLElement>(".main"), "the anchor scroller");
    const beforeFile = must(before.querySelector<HTMLDetailsElement>("details.file"), "the anchor file");
    beforeScroller.scrollTop = 83;
    setTestRect(beforeScroller, { y: 40, height: 200 });
    setTestRect(beforeFile, { y: 54, height: 80 });

    const snapshot = captureSwapAnchor(before);
    const next = viewWithFile("src/lib.rs");
    let restoredOffset: number | undefined;
    restoreSwapAnchor(next, snapshot, (_target, _scroller, offset) => {
      restoredOffset = offset;
    });

    expect(restoredOffset).toBe(14);
    expect(next.querySelector<HTMLDetailsElement>("details.file")?.open).toBe(true);
  });

  test("falls back to the clamped numeric scroll position when the anchor file disappeared", () => {
    const before = viewWithFile("src/old.rs");
    const beforeScroller = must(before.querySelector<HTMLElement>(".main"), "the anchor scroller");
    beforeScroller.scrollTop = 240;
    const snapshot = captureSwapAnchor(before);

    const next = viewWithFile("src/new.rs");
    const nextScroller = must(next.querySelector<HTMLElement>(".main"), "the replacement scroller");
    Object.defineProperties(nextScroller, {
      scrollHeight: { configurable: true, value: 150 },
      clientHeight: { configurable: true, value: 100 },
    });
    restoreSwapAnchor(next, snapshot);

    expect(nextScroller.scrollTop).toBe(50);
  });
});

test("the htmx lifecycle destroys the old view before mounting the replacement", () => {
  installSwapLifecycle(document, testLifecycle);
  const before = viewWithFile("src/lib.rs");
  document.body.appendChild(before);
  testLifecycle.enhanceWithin(before);
  const oldLayout = must(before.querySelector<HTMLElement>(".layout"), "the mounted layout");

  dispatchSwap("htmx:beforeSwap", before, before);
  expect(oldLayout.dataset["gtlEnhanced"]).toBeUndefined();

  const next = viewWithFile("src/lib.rs");
  before.remove();
  document.body.appendChild(next);
  dispatchSwap("htmx:afterSwap", next, before);
  expect(next.querySelector<HTMLElement>(".layout")?.dataset["gtlEnhanced"]).toBe("true");
  expect(oldLayout.dataset["gtlEnhanced"]).toBeUndefined();
});

test("a semantic view change resets the viewport instead of carrying a range anchor into a commit patch", () => {
  installSwapLifecycle(document, testLifecycle);
  const before = viewWithFile("src/lib.rs", "1:range");
  const beforeScroller = must(before.querySelector<HTMLElement>(".main"), "the range scroller");
  beforeScroller.scrollTop = 83;
  document.body.appendChild(before);

  dispatchSwap("htmx:beforeSwap", before, before);
  const next = viewWithFile("src/lib.rs", "1:commit:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
  const nextScroller = must(next.querySelector<HTMLElement>(".main"), "the commit scroller");
  before.remove();
  document.body.appendChild(next);
  dispatchSwap("htmx:afterSwap", next, before);

  expect(nextScroller.scrollTop).toBe(0);
  expect(next.querySelector<HTMLDetailsElement>("details.file")?.open).toBe(false);
});

test("a canceled normal swap leaves the existing view mounted and interactive", () => {
  installSwapLifecycle(document, testLifecycle);
  const view = viewWithFile("src/lib.rs");
  document.body.appendChild(view);
  testLifecycle.enhanceWithin(view);
  const root = must(view.querySelector<HTMLElement>(".layout"), "the mounted layout");
  const file = must(view.querySelector<HTMLDetailsElement>("details.file"), "the mounted file");
  const fold = must(view.querySelector<HTMLButtonElement>(".foldall"), "the mounted fold button");

  dispatchSwap("htmx:beforeSwap", view, view, false);
  expect(root.dataset["gtlEnhanced"]).toBe("true");
  fold.click();
  expect(file.open).toBe(true);
});

test.each([
  { name: "a marked recovery error opts into the real htmx swap", marker: "true", shouldSwap: true, isError: false },
  { name: "an unmarked settings-like HTTP error stays unswapped", marker: null, shouldSwap: false, isError: true },
])("$name", ({ marker, shouldSwap, isError }) => {
  installSwapLifecycle(document, testLifecycle);
  const view = viewWithFile("src/lib.rs");
  document.body.appendChild(view);
  testLifecycle.enhanceWithin(view);

  const detail = dispatchSwap("htmx:beforeSwap", view, view, false, marker);

  expect(detail.shouldSwap).toBe(shouldSwap);
  expect(detail.isError).toBe(isError);
});

test("an OOB viewer replacement unmounts the old view and enhances the inserted event origin", () => {
  installSwapLifecycle(document, testLifecycle);
  const oldView = viewWithFile("src/lib.rs");
  document.body.appendChild(oldView);
  testLifecycle.enhanceWithin(oldView);
  const oldRoot = must(oldView.querySelector<HTMLElement>(".layout"), "the mounted layout");

  dispatchSwap("htmx:oobBeforeSwap", oldView, oldView);
  expect(oldRoot.dataset["gtlEnhanced"]).toBeUndefined();

  const nextView = viewWithFile("src/lib.rs");
  oldView.remove();
  document.body.appendChild(nextView);
  dispatchSwap("htmx:oobAfterSwap", nextView, oldView);

  expect(nextView.querySelector<HTMLElement>(".layout")?.dataset["gtlEnhanced"]).toBe("true");
  expect(oldRoot.dataset["gtlEnhanced"]).toBeUndefined();
});
