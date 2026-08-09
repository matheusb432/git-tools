import { keyboardCommand } from "./model/keyboard";
import { scrollLandOn } from "./scroll";
import { createTeardown } from "./teardown";
import { enhanceDiffDocument } from "./enhance-document";

/**
 * Opens `target`, then lands on it in the next animation frame so a collapsed
 * content-visibility giant materializes before the landing correction loop runs.
 */
function navigateToFile(target: HTMLDetailsElement, scroller: HTMLElement, stickyTop: number): void {
  target.open = true;
  requestAnimationFrame(() => scrollLandOn(target, scroller, { stickyTop }));
}

export function enhanceLayout(root: HTMLElement): () => void {
  const cleanupDiffDocument = enhanceDiffDocument(root);
  const { listen, later, destroy } = createTeardown();
  // Query within `root`, never document: the tabbed view inlines one .layout per panel in a
  // single document, and document-level queries would only ever wire the first panel.
  const fileEls = Array.from(root.querySelectorAll<HTMLDetailsElement>("details.file"));
  const filePaths = fileEls.map((el) => (el.getAttribute("data-path") || "").toLowerCase());
  const treeBody = root.querySelector<HTMLElement>(".tree-body");
  const treeFileLeaves = Array.from(root.querySelectorAll<HTMLElement>(".tree-body .tfile"));
  const mobileFileMenu = root.querySelector<HTMLElement>("[data-preview-files-popover]");
  const filterInput = root.querySelector<HTMLInputElement>(".search input");
  const foldAll = root.querySelector<HTMLElement>(".foldall");
  let filterText = "";

  const mainScroller = root.querySelector<HTMLElement>(".main") ?? root;

  if (treeBody) {
    listen(treeBody, "click", (event) => {
      if (!(event.target instanceof Element)) return;
      const label = event.target.closest<HTMLElement>(".tlabel");
      if (!label || !treeBody.contains(label)) return;
      const fileItem = label.closest<HTMLElement>(".tfile");
      if (fileItem) {
        const targetId = fileItem.getAttribute("data-target");
        const file = targetId ? fileEls.find((candidate) => candidate.id === targetId) : undefined;
        if (file) openAndScrollTo(file);
        return;
      }
      label.closest<HTMLElement>(".tdir")?.classList.toggle("open");
    });
  }

  if (mobileFileMenu) {
    listen(mobileFileMenu, "click", (event) => {
      if (!(event.target instanceof Element)) return;
      const button = event.target.closest<HTMLElement>("[data-file-target]");
      if (!button || !mobileFileMenu.contains(button)) return;
      const targetId = button.getAttribute("data-file-target");
      const file = targetId ? fileEls.find((candidate) => candidate.id === targetId) : undefined;
      if (!file) return;
      openAndScrollTo(file);
      mobileFileMenu.hidePopover?.();
    });
  }

  function openAndScrollTo(t: HTMLDetailsElement): void {
    const summaryEl = t.querySelector<HTMLElement>("summary");
    const stickyTop = summaryEl ? summaryEl.offsetHeight : 0;
    navigateToFile(t, mainScroller, stickyTop);
    t.classList.add("flash");
    later(() => {
      t.classList.remove("flash");
    }, 1200);
    markCurrent(t);
  }

  function applyFilter(): void {
    filePaths.forEach((path, index) => {
      const el = fileEls[index];
      if (el) el.hidden = filterText.length > 0 && !path.includes(filterText);
    });
    treeFileLeaves.forEach((leaf) => {
      const path = leaf.getAttribute("data-path") || "";
      leaf.hidden = filterText.length > 0 && !path.includes(filterText);
    });
    Array.from(root.querySelectorAll<HTMLElement>(".tree-body .tdir"))
      .reverse()
      .forEach((directory) => {
        directory.hidden = !Array.from(directory.querySelectorAll<HTMLElement>(".tfile")).some((leaf) => !leaf.hidden);
      });
  }

  if (filterInput) {
    listen(filterInput, "input", () => {
      filterText = filterInput.value.trim().toLowerCase();
      applyFilter();
    });
  }

  if (foldAll) {
    listen(foldAll, "click", () => {
      const anyOpen = fileEls.some((el) => el.open);
      fileEls.forEach((el) => {
        el.open = !anyOpen;
      });
    });
  }

  // The context toggle drives `.copy-ctx` on this view's root; the copy button reads that
  // class at click time to decide whether to prepend the path/lines header.
  const ctxToggle = root.querySelector<HTMLElement>(".ctx-toggle");
  if (ctxToggle) {
    listen(ctxToggle, "click", () => {
      const on = root.classList.toggle("copy-ctx");
      ctxToggle.setAttribute("aria-pressed", on ? "true" : "false");
      ctxToggle.classList.toggle("active", on);
    });
  }

  listen(root, "click", (event) => {
    if (!(event.target instanceof Element)) return;
    const trigger = event.target.closest<HTMLElement>("[data-preview-action]");
    if (!trigger || !root.contains(trigger)) return;
    switch (trigger.getAttribute("data-preview-action")) {
      case "fold-all":
        foldAll?.click();
        break;
      case "toggle-context":
        ctxToggle?.click();
        break;
      default:
        return;
    }
    trigger.closest<HTMLElement>("[popover]")?.hidePopover?.();
  });

  function markCurrent(el: HTMLElement): void {
    treeFileLeaves.forEach((leaf) => {
      leaf.classList.toggle("cur", leaf.getAttribute("data-target") === el.id);
    });
  }

  let curFile = -1;

  // Scan for the wanted visible file instead of materializing the visible subset: j and k
  // must allocate nothing per keypress, however many files the diff carries.
  function focusFile(index: number): void {
    const wanted = Math.max(index, 0);
    let visibleIndex = -1;
    let target: HTMLDetailsElement | null = null;
    for (const el of fileEls) {
      if (el.hidden) continue;
      visibleIndex += 1;
      target = el;
      if (visibleIndex >= wanted) break;
    }
    if (!target) return;
    curFile = Math.min(wanted, visibleIndex);
    openAndScrollTo(target);
  }

  // Listen on document (a div gets no keydown without focus) but ignore events while this
  // layout's tabbed panel is hidden, so each panel stays independently driven.
  listen(document, "keydown", (event) => {
    if (!(event instanceof KeyboardEvent)) return;
    const panel = root.closest<HTMLElement>(".panel");
    if (panel && panel.hidden) return;
    const command = keyboardCommand(event);
    switch (command) {
      case "blur-input":
        if (event.target instanceof HTMLElement) event.target.blur();
        return;
      case "fold-all":
        event.preventDefault();
        foldAll?.click();
        return;
      case "focus-filter":
        event.preventDefault();
        filterInput?.focus();
        return;
      case "next-file":
        event.preventDefault();
        focusFile(curFile + 1);
        return;
      case "previous-file":
        event.preventDefault();
        focusFile(curFile - 1);
        return;
      case "none":
        return;
      default: {
        const unreachable: never = command;
        return unreachable;
      }
    }
  });

  return () => {
    destroy();
    cleanupDiffDocument();
  };
}
