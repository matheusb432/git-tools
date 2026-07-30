import { describe, expect, test, vi } from "vitest";
import { enhanceLayout, handleDocumentCopy } from "./enhance-layout";

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

describe("handleDocumentCopy", () => {
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

    handleDocumentCopy(event as ClipboardEvent);

    expect(document.querySelector(".gtl-toast")).not.toBeNull();
    expect(clipboardData.setData).toHaveBeenCalledWith("text/plain", "// * src/main.ts\nconst value = 1;");
  });
});

describe("enhanceLayout", () => {
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

  test("the mobile changed-files menu navigates to its server-rendered file and closes", () => {
    const root = layout("mobile");
    const file = must(root.querySelector<HTMLDetailsElement>("details.file"), "the layout file");
    file.id = "file-mobile";
    file.open = false;
    const popover = document.createElement("aside");
    popover.id = "viewer-files-popover";
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
