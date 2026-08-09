import { computeWheelScroll } from "../shared/wheel";
import { showToast } from "../shared/toast";
import { copyText } from "./clipboard";
import { enhanceControls } from "./controls";
import { toggleLongLine } from "./long-lines";
import { copyContextEnabled, copyHeader, readCopiedRows } from "./model/copy";
import { scrollLandOn } from "./scroll";
import { createTeardown } from "./teardown";

export const DIFF_DOCUMENT_SELECTOR = "[data-gtl-diff-document]";
export const OPEN_DIFF_FILE_EVENT = "gtl:open-diff-file";

export type DiffDocumentAnchor =
  | { readonly path: string; readonly offset: number; readonly scrollTop: number }
  | { readonly path: null; readonly scrollTop: number };

type LandOnAnchor = (target: HTMLDetailsElement, scroller: HTMLElement, offset: number) => void;

function diffScroller(scope: ParentNode): HTMLElement | null {
  if (scope instanceof HTMLElement && scope.matches("[data-gtl-diff-scroller], .main")) return scope;
  return (
    scope.querySelector<HTMLElement>("[data-gtl-diff-scroller], .main") ?? (scope instanceof HTMLElement ? scope : null)
  );
}

function elementByExactId(root: HTMLElement, id: string): HTMLElement | null {
  return [...root.querySelectorAll<HTMLElement>("[id]")].find((element) => element.id === id) ?? null;
}

function fileByPath(root: ParentNode, path: string): HTMLDetailsElement | null {
  return (
    [...root.querySelectorAll<HTMLDetailsElement>("details.file[data-path]")].find(
      (file) => file.getAttribute("data-path") === path,
    ) ?? null
  );
}

function navigateToFile(target: HTMLDetailsElement, scroller: HTMLElement): void {
  target.open = true;
  const stickyTop = target.querySelector<HTMLElement>("summary")?.offsetHeight ?? 0;
  requestAnimationFrame(() => scrollLandOn(target, scroller, { stickyTop }));
}

export function captureDiffDocumentAnchor(scope: ParentNode): DiffDocumentAnchor {
  const scroller = diffScroller(scope);
  if (!scroller) return { path: null, scrollTop: 0 };
  const scrollerTop = scroller.getBoundingClientRect().top;
  for (const file of scope.querySelectorAll<HTMLDetailsElement>("details.file[data-path]")) {
    if (file.hidden) continue;
    const rect = file.getBoundingClientRect();
    if (rect.bottom <= scrollerTop) continue;
    const path = file.getAttribute("data-path");
    if (path === null) continue;
    return { path, offset: rect.top - scrollerTop, scrollTop: scroller.scrollTop };
  }
  return { path: null, scrollTop: scroller.scrollTop };
}

export function restoreDiffDocumentAnchor(
  scope: ParentNode,
  anchor: DiffDocumentAnchor,
  land: LandOnAnchor = (target, scroller, offset) => scrollLandOn(target, scroller, { stickyTop: offset }),
): void {
  const scroller = diffScroller(scope);
  if (!scroller) return;
  if (anchor.path !== null) {
    const target = fileByPath(scope, anchor.path);
    if (target) {
      target.open = true;
      land(target, scroller, anchor.offset);
      return;
    }
  }
  scroller.scrollTop = Math.min(
    Math.max(anchor.scrollTop, 0),
    Math.max(scroller.scrollHeight - scroller.clientHeight, 0),
  );
}

export function scrollDiffDocumentToFile(root: HTMLElement, targetId: string): boolean {
  const target = elementByExactId(root, targetId);
  if (!(target instanceof HTMLDetailsElement) || !target.matches("details.file")) return false;
  const scroller = diffScroller(root);
  if (!scroller) return false;
  navigateToFile(target, scroller);
  return true;
}

export function setDiffDocumentFilesFolded(root: HTMLElement, folded: boolean): void {
  root.querySelectorAll<HTMLDetailsElement>("details.file").forEach((file) => {
    file.open = !folded;
  });
}

function clipboardEvent(event: Event): event is ClipboardEvent {
  return "clipboardData" in event;
}

function isShadowLocalContainer(value: unknown): value is ShadowRoot {
  return typeof value === "object" && value !== null && "host" in value && "appendChild" in value;
}

export function handleDiffDocumentCopy(event: Event, root: HTMLElement): void {
  if (!clipboardEvent(event)) return;
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || !selection.rangeCount || !selection.containsNode) return;
  const node = selection.getRangeAt(0).commonAncestorContainer;
  const element = node instanceof Element ? node : node.parentElement;
  const file = element?.closest ? element.closest("details.file") : null;
  if (!file || !root.contains(file) || !copyContextEnabled(file)) return;
  const diffBlock = element?.closest ? element.closest(".diff") : null;
  if (!diffBlock || !root.contains(diffBlock)) return;
  const copied = readCopiedRows(diffBlock.querySelectorAll(".dl-add, .dl-ctx"), (row) =>
    selection.containsNode(row, true),
  );
  if (!copied.lines.length || !event.clipboardData) return;
  event.clipboardData.setData("text/plain", `${copyHeader(file, copied.lineRange)}\n${copied.lines.join("\n")}`);
  event.preventDefault();
  const treeRoot = typeof root.getRootNode === "function" ? root.getRootNode() : null;
  const toastContainer = isShadowLocalContainer(treeRoot) ? treeRoot : document.body;
  showToast(
    copied.lineRange === null ? "Copied with context" : `Copied with context - lines ${copied.lineRange}`,
    toastContainer,
  );
}

function popoverById(root: HTMLElement, id: string): HTMLElement | null {
  const popover = elementByExactId(root, id);
  return popover?.matches("[popover]") ? popover : null;
}

function openDiffFilePath(trigger: HTMLElement): string | null {
  const ownPath = trigger.getAttribute("data-open-diff-file");
  if (ownPath) return ownPath;
  return trigger.closest("details.file[data-path]")?.getAttribute("data-path") ?? null;
}

export function enhanceDiffDocument(root: HTMLElement): () => void {
  const cleanupControls = enhanceControls(root);
  const { listen, later, cancel, destroy } = createTeardown();
  let activePopover: HTMLElement | null = null;
  let hidePopoverTimer: ReturnType<typeof setTimeout> | undefined;

  const cancelPopoverHide = (): void => {
    if (hidePopoverTimer === undefined) return;
    cancel(hidePopoverTimer);
    hidePopoverTimer = undefined;
  };
  const hidePopover = (): void => {
    cancelPopoverHide();
    hidePopoverTimer = later(() => {
      activePopover?.hidePopover?.();
      activePopover = null;
      hidePopoverTimer = undefined;
    }, 140);
  };

  listen(root, "copy", (event) => handleDiffDocumentCopy(event, root));
  listen(root, "click", (event) => {
    if (!(event.target instanceof Element)) return;
    const longLineButton = event.target.closest<HTMLElement>(".ln-more");
    if (longLineButton && root.contains(longLineButton)) {
      toggleLongLine(longLineButton);
      return;
    }

    const hashCopy = event.target.closest<HTMLElement>(".cline[data-sha] .sha");
    if (hashCopy && root.contains(hashCopy)) {
      const commitLine = hashCopy.closest<HTMLElement>(".cline[data-sha]");
      if (!commitLine) return;
      const sha = commitLine.getAttribute("data-sha");
      if (sha === null) return;
      event.stopPropagation();
      void copyText(sha);
      commitLine.classList.add("copied");
      later(() => commitLine.classList.remove("copied"), 900);
      return;
    }

    const openFile = event.target.closest<HTMLElement>("[data-open-diff-file]");
    if (!openFile || !root.contains(openFile)) return;
    const path = openDiffFilePath(openFile);
    if (path === null) return;
    event.preventDefault();
    openFile.dispatchEvent(
      new CustomEvent(OPEN_DIFF_FILE_EVENT, {
        bubbles: true,
        composed: true,
        detail: { path },
      }),
    );
  });

  listen(
    root,
    "wheel",
    (event) => {
      if (!(event instanceof WheelEvent) || !(event.target instanceof Element)) return;
      const scroller = event.target.closest<HTMLElement>(".diff");
      if (!scroller || !root.contains(scroller)) return;
      const next = computeWheelScroll(scroller, event);
      if (next === null) return;
      event.stopPropagation();
      event.preventDefault();
      scroller.scrollLeft = next;
    },
    { passive: false },
  );

  listen(root, "mouseover", (event) => {
    if (!(event.target instanceof Element)) return;
    const popover = event.target.closest<HTMLElement>("[popover]");
    if (popover && popover === activePopover) {
      cancelPopoverHide();
      return;
    }
    const commitLine = event.target.closest<HTMLElement>(".cline[data-pop]");
    if (!commitLine || !root.contains(commitLine)) return;
    const popoverId = commitLine.getAttribute("data-pop");
    const nextPopover = popoverId ? popoverById(root, popoverId) : null;
    if (!nextPopover) return;
    cancelPopoverHide();
    activePopover?.hidePopover?.();
    activePopover = nextPopover;
    const rectangle = commitLine.getBoundingClientRect();
    let left = rectangle.left - 338;
    if (left < 8) left = rectangle.right + 6;
    nextPopover.style.left = `${left}px`;
    nextPopover.style.top = `${Math.min(rectangle.top, window.innerHeight - 210)}px`;
    nextPopover.showPopover?.();
  });
  listen(root, "mouseout", (event) => {
    if (!(event.target instanceof Element)) return;
    if (event.target.closest(".cline[data-pop]") || event.target.closest("[popover]")) hidePopover();
  });

  return () => {
    destroy();
    cleanupControls();
  };
}
