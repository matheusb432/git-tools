import { expect, test, vi } from "vitest";
import { enhanceLayout, handleDocumentCopy } from "./enhance-layout";
import { enhanceControls } from "./controls";
import { resolveActiveSet } from "./commit-focus";
import { buildFileLeaf } from "./file-tree";
import { toggleLongLine } from "./long-lines";
import { captureSwapAnchor, createEnhancementLifecycle, installSwapLifecycle, restoreSwapAnchor } from "./swap";
import { initTabs } from "./tabbed";

// JS behavior contracts migrated from deleted Rust PREVIEW_JS.contains tests.
// Covered here so the frontend unit suite owns the JS logic while cargo test stays JS-free.

function must<T>(value: T | null, what: string): T {
  if (value === null) throw new Error(`${what} is missing`);
  return value;
}

test("non-merge card: active set is [sha]", () => {
  const focus = must(resolveActiveSet("abc123def", "", null), "the commit focus");
  expect(focus.sha).toBe("abc123def");
  expect([...focus.shas]).toEqual(["abc123def"]);
});

test("merge card: active set is the member list", () => {
  const focus = must(resolveActiveSet("merge1234", "aaa111aaa bbb222bbb", null), "the commit focus");
  expect(focus.sha).toBe("merge1234");
  expect([...focus.shas]).toEqual(["aaa111aaa", "bbb222bbb"]);
});

test("toggling the same sha clears focus", () => {
  expect(resolveActiveSet("abc123def", "", "abc123def")).toBeNull();
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

test("file tree leaves emit semantic hooks without presentation utilities", () => {
  const leaf = buildFileLeaf(document, {
    name: "main.ts",
    status: "added",
    statusCode: "A",
    statusLabel: "Added file",
    el: { id: "file-main-ts" },
  });

  expect(leaf.classList.contains("tfile")).toBe(true);
  expect(leaf.classList.contains("status-added")).toBe(true);
  expect(leaf.getAttribute("data-target")).toBe("file-main-ts");
  expect(must(leaf.querySelector<HTMLElement>(".tlabel"), "the leaf label").className).toBe("tlabel");
  expect(must(leaf.querySelector<HTMLElement>(".tname"), "the leaf name").className).toBe("tname");
  expect(must(leaf.querySelector<HTMLElement>(".tstatus"), "the leaf status").className).toBe("tstatus status-added");
});

test("tab hooks select one semantic panel at a time", () => {
  const tabs = document.createElement("nav");
  tabs.className = "tabs";
  const tab0 = document.createElement("button");
  tab0.className = "tab active";
  tab0.id = "tab-0";
  tab0.setAttribute("data-tab", "0");
  tab0.setAttribute("aria-selected", "true");
  const tab1 = document.createElement("button");
  tab1.className = "tab";
  tab1.id = "tab-1";
  tab1.setAttribute("data-tab", "1");
  tab1.setAttribute("aria-selected", "false");
  tabs.appendChild(tab0);
  tabs.appendChild(tab1);
  const panel0 = document.createElement("section");
  panel0.className = "panel";
  panel0.id = "panel-0";
  const panel1 = document.createElement("section");
  panel1.className = "panel";
  panel1.id = "panel-1";
  panel1.hidden = true;
  document.body.appendChild(tabs);
  document.body.appendChild(panel0);
  document.body.appendChild(panel1);

  initTabs();
  tab1.click();

  expect(tab0.getAttribute("aria-selected")).toBe("false");
  expect(tab1.getAttribute("aria-selected")).toBe("true");
  expect(panel0.hidden).toBe(true);
  expect(panel1.hidden).toBe(false);
});

test("a diff selection copies as headed source and announces itself", () => {
  const root = document.createElement("div");
  root.className = "layout copy-ctx";
  const file = document.createElement("details");
  file.className = "file";
  file.setAttribute("data-comment", "//");
  file.setAttribute("data-path", "src/main.ts");
  const diff = document.createElement("div");
  diff.className = "diff";
  const row = document.createElement("div");
  row.className = "dl-add";
  const code = document.createElement("code");
  code.textContent = "+const value = 1;";
  row.appendChild(code);
  diff.appendChild(row);
  file.appendChild(diff);
  root.appendChild(file);
  document.body.appendChild(root);
  Object.defineProperty(window, "getSelection", {
    configurable: true,
    value: () => ({
      isCollapsed: false,
      rangeCount: 1,
      containsNode: () => true,
      getRangeAt: () => ({ commonAncestorContainer: code }),
    }),
  });
  const clipboardData = { setData: vi.fn() };
  const event = new Event("copy", { cancelable: true });
  Object.defineProperty(event, "clipboardData", { value: clipboardData });

  handleDocumentCopy(event as ClipboardEvent);

  expect(document.querySelector(".gtl-toast")).not.toBeNull();
  expect(clipboardData.setData).toHaveBeenCalledWith("text/plain", "// * src/main.ts\nconst value = 1;");
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

test("control cleanup removes button behavior before a swapped layout is discarded", async () => {
  const root = document.createElement("div");
  const button = document.createElement("button");
  button.className = "copy-button";
  button.dataset["copyValue"] = "abc123";
  button.dataset["copyLabel"] = "copy";
  root.appendChild(button);
  const cleanup = enhanceControls(root);

  button.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(button.dataset["state"]).toBe("err");

  cleanup();
  button.dataset["state"] = "";
  button.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(button.dataset["state"]).toBe("");
});

test("a path segment named after an Object.prototype member nests like any other directory", () => {
  const root = layout("proto");
  const file = must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file");
  file.id = "file-proto";
  file.setAttribute("data-path", "constructor/toString/main.rs");
  const treeBody = document.createElement("div");
  treeBody.className = "tree-body";
  root.appendChild(treeBody);

  const cleanup = enhanceLayout(root);

  const dirNames = [...treeBody.querySelectorAll<HTMLElement>(".tdir")].map(
    (dir) => must(dir.querySelector<HTMLElement>(".tname"), "the directory name").textContent,
  );
  expect(dirNames).toEqual(["constructor", "toString"]);
  const leaf = must(treeBody.querySelector<HTMLElement>(".tfile"), "the file leaf");
  expect(leaf.getAttribute("data-target")).toBe("file-proto");
  expect(must(leaf.querySelector<HTMLElement>(".tname"), "the leaf name").textContent).toBe("main.rs");
  cleanup();
});

test("rebuilding the file tree detaches old labels while the current tree still navigates", () => {
  const root = layout("alpha");
  const alpha = must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file");
  alpha.id = "file-alpha";
  alpha.setAttribute("data-path", "src/alpha.rs");
  alpha.setAttribute("data-commits", "aaa111aaa");
  const beta = document.createElement("details");
  beta.className = "file";
  beta.id = "file-beta";
  beta.setAttribute("data-path", "src/beta.rs");
  beta.setAttribute("data-commits", "bbb222bbb");
  must(root.querySelector<HTMLElement>(".main"), "the layout scroller").appendChild(beta);
  const search = document.createElement("div");
  search.className = "search";
  const filter = document.createElement("input");
  search.appendChild(filter);
  const treeBody = document.createElement("div");
  treeBody.className = "tree-body";
  const commit = document.createElement("div");
  commit.className = "cline";
  commit.setAttribute("data-sha", "aaa111aaa");
  root.appendChild(search);
  root.appendChild(treeBody);
  root.appendChild(commit);

  const cleanup = enhanceLayout(root);
  const staleLabel = must(treeBody.querySelector<HTMLElement>(".tfile .tlabel"), "the initial tree label");
  for (const query of ["alpha", "", "beta", "", "alpha", ""]) {
    filter.value = query;
    filter.dispatchEvent(new Event("input", { bubbles: true }));
  }
  commit.click();
  commit.click();

  staleLabel.click();
  expect(alpha.open).toBe(false);
  expect(beta.open).toBe(false);
  const currentBeta = must(
    treeBody.querySelector<HTMLElement>('.tfile[data-target="file-beta"] .tlabel'),
    "the rebuilt beta leaf",
  );
  currentBeta.click();
  expect(beta.open).toBe(true);
  cleanup();
});
