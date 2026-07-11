import { expect, test } from "bun:test";
import { navigateToFile } from "./enhance-layout";
import { enhanceLayout } from "./enhance-layout";
import { enhanceControls } from "./controls";
import { resolveActiveSet } from "./commit-focus";
import { toggleLongLine } from "./long-lines";
import {
  captureSwapAnchor,
  createEnhancementLifecycle,
  installSwapLifecycle,
  restoreSwapAnchor,
} from "./swap";

// JS behavior contracts migrated from deleted Rust PREVIEW_JS.contains tests.
// Covered here so bun test owns the JS logic while cargo test stays bun-free.

test("non-merge card: active set is [sha]", () => {
  const result = resolveActiveSet("abc123def", "", null);
  expect(result.sha).toBe("abc123def");
  expect(result.set).toEqual(["abc123def"]);
});

test("merge card: active set is the member list", () => {
  const result = resolveActiveSet("merge1234", "aaa111aaa bbb222bbb", null);
  expect(result.sha).toBe("merge1234");
  expect(result.set).toEqual(["aaa111aaa", "bbb222bbb"]);
});

test("toggling the same sha clears focus", () => {
  const result = resolveActiveSet("abc123def", "", "abc123def");
  expect(result.sha).toBeNull();
  expect(result.set).toBeNull();
});

test("navigateToFile opens the target (materializing a collapsed giant) before landing", () => {
  let opened = false;
  let landed = false;
  const target = document.createElement("details");
  const scroller = document.createElement("main");
  Object.defineProperty(target, "open", {
    configurable: true,
    get: () => opened,
    set: (value: boolean) => {
      opened = value;
    },
  });
  navigateToFile(target, scroller, {
    raf: (cb) => {
      cb(0);
      return 0;
    },
    land: () => {
      landed = opened;
    }, // assert open happened first
  });
  expect(opened).toBe(true);
  expect(landed).toBe(true);
});

test("toggleLongLine flips expanded + aria on the owning row", () => {
  const row = document.createElement("div");
  row.className = "dl-long";
  const btn = document.createElement("button");
  row.appendChild(btn);
  toggleLongLine(btn);
  expect(row.classList.contains("expanded")).toBe(true);
  expect(btn.getAttribute("aria-expanded")).toBe("true");
  // second click collapses again
  toggleLongLine(btn);
  expect(row.classList.contains("expanded")).toBe(false);
  expect(btn.getAttribute("aria-expanded")).toBe("false");
});

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

function viewWithFile(path: string): HTMLElement {
  const view = document.createElement("section");
  view.id = "viewer-view";
  const root = layout("view");
  const file = root.querySelector<HTMLDetailsElement>("details.file");
  if (!file) throw new Error("layout helper must include a file");
  file.setAttribute("data-path", path);
  view.appendChild(root);
  return view;
}

type SwapEventName = "htmx:beforeSwap" | "htmx:afterSwap" | "htmx:oobBeforeSwap" | "htmx:oobAfterSwap";

function dispatchSwap(type: SwapEventName, origin: HTMLElement, detailTarget: HTMLElement, shouldSwap = true): void {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "detail", { value: { target: detailTarget, shouldSwap } });
  origin.dispatchEvent(event);
}

const testLifecycle = createEnhancementLifecycle(
  (root) => ({ cleanup: enhanceLayout(root) }),
  (component) => component.cleanup(),
);

test("enhances a layout once, tears it down, and enhances a swapped subtree once", async () => {
  document.body.replaceChildren();
  const first = layout("first");
  document.body.appendChild(first);
  testLifecycle.enhanceWithin(first);
  testLifecycle.enhanceWithin(first);
  await Promise.resolve();
  expect(first.dataset["gtlEnhanced"]).toBe("true");

  const firstFile = first.querySelector<HTMLDetailsElement>("details.file");
  const firstFold = first.querySelector<HTMLButtonElement>(".foldall");
  if (!firstFile || !firstFold) throw new Error("layout helper is incomplete");
  firstFold.click();
  expect(firstFile.open).toBe(true);
  testLifecycle.destroyWithin(first);
  firstFold.click();
  expect(firstFile.open).toBe(true);
  expect(first.dataset["gtlEnhanced"]).toBeUndefined();

  installSwapLifecycle(document, testLifecycle);
  installSwapLifecycle(document, testLifecycle);
  const requestTarget = document.createElement("section");
  requestTarget.id = "viewer-view";
  const replacementView = document.createElement("section");
  replacementView.id = "viewer-view";
  const replacement = layout("replacement");
  replacementView.appendChild(replacement);
  document.body.appendChild(replacementView);
  dispatchSwap("htmx:afterSwap", replacementView, requestTarget);
  await Promise.resolve();
  expect(replacement.dataset["gtlEnhanced"]).toBe("true");
  const replacementFile = replacement.querySelector<HTMLDetailsElement>("details.file");
  const replacementFold = replacement.querySelector<HTMLButtonElement>(".foldall");
  if (!replacementFile || !replacementFold) throw new Error("replacement helper is incomplete");
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
  const secondFile = second.querySelector<HTMLDetailsElement>("details.file");
  const secondFold = second.querySelector<HTMLButtonElement>(".foldall");
  if (!secondFile || !secondFold) throw new Error("second replacement helper is incomplete");
  secondFold.click();
  expect(secondFile.open).toBe(true);
});

test("restores the top visible file, opens it, and preserves its viewport offset", () => {
  const before = viewWithFile("src/lib.rs");
  const beforeScroller = before.querySelector<HTMLElement>(".main");
  const beforeFile = before.querySelector<HTMLDetailsElement>("details.file");
  if (!beforeScroller || !beforeFile) throw new Error("view helper is incomplete");
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
  const beforeScroller = before.querySelector<HTMLElement>(".main");
  if (!beforeScroller) throw new Error("view helper is incomplete");
  beforeScroller.scrollTop = 240;
  const snapshot = captureSwapAnchor(before);

  const next = viewWithFile("src/new.rs");
  const nextScroller = next.querySelector<HTMLElement>(".main");
  if (!nextScroller) throw new Error("view helper is incomplete");
  Object.defineProperties(nextScroller, {
    scrollHeight: { configurable: true, value: 150 },
    clientHeight: { configurable: true, value: 100 },
  });
  restoreSwapAnchor(next, snapshot);

  expect(nextScroller.scrollTop).toBe(50);
});

test("the htmx lifecycle destroys the old view before mounting the replacement", async () => {
  document.body.replaceChildren();
  installSwapLifecycle(document, testLifecycle);
  const before = viewWithFile("src/lib.rs");
  document.body.appendChild(before);
  testLifecycle.enhanceWithin(before);
  await Promise.resolve();
  const oldLayout = before.querySelector<HTMLElement>(".layout");
  if (!oldLayout) throw new Error("view helper is incomplete");

  dispatchSwap("htmx:beforeSwap", before, before);
  expect(oldLayout.dataset["gtlEnhanced"]).toBeUndefined();

  const next = viewWithFile("src/lib.rs");
  before.remove();
  document.body.appendChild(next);
  dispatchSwap("htmx:afterSwap", next, before);
  await Promise.resolve();
  expect(next.querySelector<HTMLElement>(".layout")?.dataset["gtlEnhanced"]).toBe("true");
  expect(oldLayout.dataset["gtlEnhanced"]).toBeUndefined();
});

test("a canceled normal swap leaves the existing view mounted and interactive", () => {
  document.body.replaceChildren();
  installSwapLifecycle(document, testLifecycle);
  const view = viewWithFile("src/lib.rs");
  document.body.appendChild(view);
  testLifecycle.enhanceWithin(view);
  const root = view.querySelector<HTMLElement>(".layout");
  const file = view.querySelector<HTMLDetailsElement>("details.file");
  const fold = view.querySelector<HTMLButtonElement>(".foldall");
  if (!root || !file || !fold) throw new Error("view helper is incomplete");

  dispatchSwap("htmx:beforeSwap", view, view, false);
  expect(root.dataset["gtlEnhanced"]).toBe("true");
  fold.click();
  expect(file.open).toBe(true);
});

test("an OOB viewer replacement unmounts the old view and enhances the inserted event origin", () => {
  document.body.replaceChildren();
  installSwapLifecycle(document, testLifecycle);
  const oldView = viewWithFile("src/lib.rs");
  document.body.appendChild(oldView);
  testLifecycle.enhanceWithin(oldView);
  const oldRoot = oldView.querySelector<HTMLElement>(".layout");
  const oldScroller = oldView.querySelector<HTMLElement>(".main");
  const oldFile = oldView.querySelector<HTMLDetailsElement>("details.file");
  if (!oldRoot || !oldScroller || !oldFile) throw new Error("old OOB view is incomplete");
  setTestRect(oldScroller, { y: 40, height: 200 });
  setTestRect(oldFile, { y: 52, height: 80 });

  const oldTabs = document.createElement("section");
  oldTabs.id = "viewer-tabs";
  const newTabs = document.createElement("section");
  newTabs.id = "viewer-tabs";
  document.body.appendChild(oldTabs);
  dispatchSwap("htmx:beforeSwap", oldTabs, oldTabs);
  oldTabs.remove();
  document.body.appendChild(newTabs);
  dispatchSwap("htmx:afterSwap", newTabs, oldTabs);

  dispatchSwap("htmx:oobBeforeSwap", oldView, oldView);
  expect(oldRoot.dataset["gtlEnhanced"]).toBeUndefined();
  const nextView = viewWithFile("src/lib.rs");
  oldView.remove();
  document.body.appendChild(nextView);
  dispatchSwap("htmx:oobAfterSwap", nextView, oldView);

  const nextRoot = nextView.querySelector<HTMLElement>(".layout");
  const nextFile = nextView.querySelector<HTMLDetailsElement>("details.file");
  const nextFold = nextView.querySelector<HTMLButtonElement>(".foldall");
  if (!nextRoot || !nextFile || !nextFold) throw new Error("new OOB view is incomplete");
  expect(nextRoot.dataset["gtlEnhanced"]).toBe("true");
  expect(oldRoot.dataset["gtlEnhanced"]).toBeUndefined();
  expect(nextFile.open).toBe(true);
  nextFile.open = false;
  nextFold.click();
  expect(nextFile.open).toBe(true);
});

test("control cleanup removes button behavior before a swapped layout is discarded", async () => {
  const root = document.createElement("div");
  const button = document.createElement("button");
  button.className = "copy-button";
  button.dataset["copyValue"] = "abc123";
  button.dataset["copyLabel"] = "copy";
  root.appendChild(button);
  const cleanup = enhanceControls(root);

  button.click();
  await Bun.sleep(0);
  expect(button.dataset["state"]).toBe("err");

  cleanup();
  button.dataset["state"] = "";
  button.click();
  await Bun.sleep(0);
  expect(button.dataset["state"]).toBe("");
});

test("rebuilding the file tree detaches old labels while the current tree still navigates", () => {
  const root = document.createElement("div");
  root.className = "layout";
  const search = document.createElement("div");
  search.className = "search";
  const filter = document.createElement("input");
  search.appendChild(filter);
  const treeBody = document.createElement("div");
  treeBody.className = "tree-body";
  const main = document.createElement("main");
  main.className = "main";
  const alpha = document.createElement("details");
  alpha.className = "file";
  alpha.id = "file-alpha";
  alpha.setAttribute("data-path", "src/alpha.rs");
  alpha.setAttribute("data-commits", "aaa111aaa");
  const beta = document.createElement("details");
  beta.className = "file";
  beta.id = "file-beta";
  beta.setAttribute("data-path", "src/beta.rs");
  beta.setAttribute("data-commits", "bbb222bbb");
  main.appendChild(alpha);
  main.appendChild(beta);
  const commit = document.createElement("div");
  commit.className = "cline";
  commit.setAttribute("data-sha", "aaa111aaa");
  root.appendChild(search);
  root.appendChild(treeBody);
  root.appendChild(main);
  root.appendChild(commit);

  const cleanup = enhanceLayout(root);
  const staleLabel = treeBody.querySelector<HTMLElement>(".tfile .tlabel");
  if (!staleLabel) throw new Error("initial tree was not rendered");
  for (const query of ["alpha", "", "beta", "", "alpha", ""]) {
    filter.value = query;
    filter.dispatchEvent(new Event("input", { bubbles: true }));
  }
  commit.click();
  commit.click();

  staleLabel.click();
  expect(alpha.open).toBe(false);
  expect(beta.open).toBe(false);
  const currentBeta = treeBody.querySelector<HTMLElement>('.tfile[data-target="file-beta"] .tlabel');
  if (!currentBeta) throw new Error("rebuilt tree is missing beta");
  currentBeta.click();
  expect(beta.open).toBe(true);
  cleanup();
});
