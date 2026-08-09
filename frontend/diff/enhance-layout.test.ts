import { describe, expect, test, vi } from "vitest";
import { enhanceLayout } from "./enhance-layout";
import { handleDiffDocumentCopy, OPEN_DIFF_FILE_EVENT } from "./enhance-document";

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

describe("handleDiffDocumentCopy", () => {
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
    code.textContent = "const value = 1;";
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

    handleDiffDocumentCopy(event, root);

    expect(document.querySelector(".gtl-toast")).not.toBeNull();
    expect(clipboardData.setData).toHaveBeenCalledWith("text/plain", "// * src/main.ts\nconst value = 1;");
  });
});

describe("enhanceLayout", () => {
  test("a retained file action emits its path through the composed host boundary", () => {
    const root = layout("open-file");
    const file = must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file");
    file.setAttribute("data-path", "src/open-file.rs");
    const button = document.createElement("button");
    button.setAttribute("data-open-diff-file", "src/open-file.rs");
    file.appendChild(button);
    let openedPath: string | null = null;
    let composed = false;
    root.addEventListener(OPEN_DIFF_FILE_EVENT, (event) => {
      composed = event.composed;
      if ("detail" in event && typeof event.detail === "object" && event.detail !== null && "path" in event.detail) {
        openedPath = typeof event.detail.path === "string" ? event.detail.path : null;
      }
    });

    const cleanup = enhanceLayout(root);
    button.click();

    expect(openedPath).toBe("src/open-file.rs");
    expect(composed).toBe(true);
    cleanup();
  });

  test("shared mobile diff actions drive their own server-rendered layout", () => {
    const root = layout("mobile-actions");
    root.classList.add("copy-ctx");
    const file = must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file");
    file.open = true;
    const context = document.createElement("button");
    context.className = "ctx-toggle active";
    context.setAttribute("aria-pressed", "true");
    const menu = document.createElement("aside");
    menu.setAttribute("popover", "");
    const foldAction = document.createElement("button");
    foldAction.setAttribute("data-preview-action", "fold-all");
    const contextAction = document.createElement("button");
    contextAction.setAttribute("data-preview-action", "toggle-context");
    menu.appendChild(foldAction);
    menu.appendChild(contextAction);
    root.appendChild(context);
    root.appendChild(menu);

    const cleanup = enhanceLayout(root);
    foldAction.click();
    contextAction.click();

    expect(file.open).toBe(false);
    expect(root.classList.contains("copy-ctx")).toBe(false);
    expect(context.getAttribute("aria-pressed")).toBe("false");
    cleanup();
  });

  test("server-rendered tree labels navigate without rebuilding the tree", () => {
    const root = layout("tree");
    const file = must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file");
    file.id = "file-tree";
    file.open = false;
    const treeBody = document.createElement("div");
    treeBody.className = "tree-body";
    const directory = document.createElement("li");
    directory.className = "tdir open";
    const directoryLabel = document.createElement("div");
    directoryLabel.className = "tlabel";
    const leaf = document.createElement("li");
    leaf.className = "tfile";
    leaf.setAttribute("data-target", file.id);
    leaf.setAttribute("data-path", "src/tree.rs");
    const leafLabel = document.createElement("div");
    leafLabel.className = "tlabel";
    leaf.appendChild(leafLabel);
    directory.appendChild(directoryLabel);
    directory.appendChild(leaf);
    treeBody.appendChild(directory);
    root.appendChild(treeBody);

    const cleanup = enhanceLayout(root);

    directoryLabel.click();
    leafLabel.click();

    expect(directory.classList.contains("open")).toBe(false);
    expect(file.open).toBe(true);
    expect(treeBody.querySelector(".tfile")).toBe(leaf);
    cleanup();
  });

  test("file search hides paired server-rendered file and tree elements", () => {
    const root = layout("alpha");
    const alpha = must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file");
    alpha.id = "file-alpha";
    alpha.setAttribute("data-path", "src/alpha.rs");
    const beta = document.createElement("details");
    beta.className = "file";
    beta.id = "file-beta";
    beta.setAttribute("data-path", "src/beta.rs");
    must(root.querySelector<HTMLElement>(".main"), "the layout scroller").appendChild(beta);
    const search = document.createElement("div");
    search.className = "search";
    const filter = document.createElement("input");
    search.appendChild(filter);
    const treeBody = document.createElement("div");
    treeBody.className = "tree-body";
    const directory = document.createElement("li");
    directory.className = "tdir open";
    for (const [path, target] of [
      ["src/alpha.rs", alpha.id],
      ["src/beta.rs", beta.id],
    ] as const) {
      const leaf = document.createElement("li");
      leaf.className = "tfile";
      leaf.setAttribute("data-path", path);
      leaf.setAttribute("data-target", target);
      directory.appendChild(leaf);
    }
    treeBody.appendChild(directory);
    root.appendChild(search);
    root.appendChild(treeBody);

    const cleanup = enhanceLayout(root);
    filter.value = "beta";
    filter.dispatchEvent(new Event("input", { bubbles: true }));

    expect(alpha.hidden).toBe(true);
    expect(beta.hidden).toBe(false);
    expect(treeBody.querySelector<HTMLElement>('[data-target="file-alpha"]')?.hidden).toBe(true);
    expect(treeBody.querySelector<HTMLElement>('[data-target="file-beta"]')?.hidden).toBe(false);
    expect(directory.hidden).toBe(false);
    cleanup();
  });

  test("the mobile changed-files menu navigates to its server-rendered file and closes", () => {
    const root = layout("mobile");
    const file = must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file");
    file.id = "file-mobile";
    file.open = false;
    const popover = document.createElement("aside");
    popover.id = "preview-files-popover-3";
    popover.setAttribute("data-preview-files-popover", "");
    const hidePopover = vi.fn();
    Object.defineProperty(popover, "hidePopover", { value: hidePopover });
    const button = document.createElement("button");
    button.setAttribute("data-file-target", file.id);
    popover.appendChild(button);
    root.appendChild(popover);

    const cleanup = enhanceLayout(root);
    button.click();

    expect(file.open).toBe(true);
    expect(hidePopover).toHaveBeenCalledOnce();
    cleanup();
  });
});
