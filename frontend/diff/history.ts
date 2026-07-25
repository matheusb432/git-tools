import { copyText } from "../core/clipboard";
import { installOnce } from "../core/install-once";

const COPY_FLASH_MS = 1200;

type HistoryClick =
  | { readonly kind: "copy"; readonly payload: string }
  | { readonly kind: "select" }
  | { readonly kind: "open" }
  | { readonly kind: "ignore" };

export function classifyHistoryClick(
  copyButton: Element | null,
  row: Element | null,
  selectingInRow: boolean,
): HistoryClick {
  if (copyButton) return { kind: "copy", payload: copyButton.getAttribute("data-history-copy") ?? "" };
  if (!row) return { kind: "ignore" };
  return selectingInRow ? { kind: "select" } : { kind: "open" };
}

// Rows are server-rendered role=button divs carrying an hx-get. A plain click opens the render;
// the copy button and text-drag selections must not. Listening in the capture phase stops those
// before the row's bubbling hx-get fires.
export const installHistoryActions = installOnce((root: Document): void => {
  root.addEventListener("click", (event) => onClick(root, event), true);
  root.addEventListener("keydown", onKeydown);
});

function onClick(root: Document, event: MouseEvent): void {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const copyButton = target.closest<HTMLElement>("[data-history-copy]");
  const row = target.closest<HTMLElement>(".viewer-history-row");
  const action = classifyHistoryClick(copyButton, row, selectionWithin(row));

  switch (action.kind) {
    case "copy":
      event.stopPropagation();
      event.preventDefault();
      if (copyButton) void copyText(action.payload).then((copied) => copied && flashCopied(copyButton));
      return;
    case "select":
      event.stopPropagation();
      return;
    case "open":
      root.getElementById("viewer-history-popover")?.hidePopover?.();
      return;
    case "ignore":
      return;
    default: {
      const unreachable: never = action;
      return unreachable;
    }
  }
}

// A div with role=button gets no native key activation.
function onKeydown(event: KeyboardEvent): void {
  if (event.key !== "Enter" && event.key !== " ") return;
  const row = event.target;
  if (!(row instanceof HTMLElement) || !row.classList.contains("viewer-history-row")) return;
  event.preventDefault();
  row.click();
}

// A selection whose range sits inside the row means the click ended a text drag, not an open.
function selectionWithin(row: Element | null): boolean {
  if (!row) return false;
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return false;
  const node = selection.getRangeAt(0).commonAncestorContainer;
  const element = node instanceof Element ? node : node.parentElement;
  return element !== null && row.contains(element);
}

function flashCopied(button: HTMLElement): void {
  button.setAttribute("data-copied", "");
  setTimeout(() => button.removeAttribute("data-copied"), COPY_FLASH_MS);
}
