import { copyText } from "../core/clipboard";
import { copyContextEnabled, copyHeader, readCopiedRows } from "../core/copy";
import { keyboardCommand } from "../core/keyboard";
import { scrollLandOn } from "../core/scroll";
import { createTeardown } from "../core/teardown";
import { showToast } from "../core/toast";
import { type CommitFocus, isShaTarget, resolveActiveSet } from "./commit-focus";
import { planFileVisibility } from "./file-filter";
import { buildFileLeaf } from "./file-tree";
import { toggleLongLine } from "./long-lines";
import { computeWheelScroll } from "../core/wheel";

/**
 * Rebuilds a native copy of a selection inside a diff as clean source (whole lines, markers
 * stripped) under the same commented "path, lines" header the copy button emits. Falls back
 * to the native copy for selections that are empty, span no single diff file, or touch no
 * code rows, and when the view's context toggle is off.
 */
export function handleDocumentCopy(e: ClipboardEvent): void {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || !sel.rangeCount || !sel.containsNode) return;
  const node = sel.getRangeAt(0).commonAncestorContainer;
  const el = node instanceof Element ? node : node.parentElement;
  const file = el?.closest ? el.closest("details.file") : null;
  if (!file || !copyContextEnabled(file)) return;
  // Pane visibility is CSS-driven, so read rows from the pane the selection sits in. Split
  // panes carry no .dl-add rows; their selections fall through to the native copy.
  const diffBlock = el?.closest ? el.closest(".diff") : null;
  if (!diffBlock) return;
  const copied = readCopiedRows(diffBlock.querySelectorAll(".dl-add, .dl-ctx"), (row) => sel.containsNode(row, true));
  if (!copied.lines.length || !e.clipboardData) return;
  e.clipboardData.setData("text/plain", `${copyHeader(file, copied.lineRange)}\n${copied.lines.join("\n")}`);
  e.preventDefault();
  showToast(copied.lineRange === null ? "Copied with context" : `Copied with context · lines ${copied.lineRange}`);
}

/**
 * Opens `target`, then lands on it in the next animation frame so a collapsed
 * content-visibility giant materializes before the landing correction loop runs.
 */
function navigateToFile(target: HTMLDetailsElement, scroller: HTMLElement, stickyTop: number): void {
  target.open = true;
  requestAnimationFrame(() => scrollLandOn(target, scroller, { stickyTop }));
}

export function enhanceLayout(root: HTMLElement): () => void {
  const { listen, later, cancel, destroy } = createTeardown();
  // Query within `root`, never document: the tabbed view inlines one .layout per panel in a
  // single document, and document-level queries would only ever wire the first panel.
  const fileEls = Array.from(root.querySelectorAll<HTMLDetailsElement>("details.file"));
  const clineEls = Array.from(root.querySelectorAll<HTMLElement>(".cline[data-sha]"));
  const dlEls = Array.from(
    root.querySelectorAll<HTMLElement>(".dl-add[data-commit],.dl-del[data-commit],.sp[data-commit]"),
  );
  // The server-rendered attributes never change, so read them once per enhance
  // instead of on every filter pass.
  const dlCommits = dlEls.map((row) => row.getAttribute("data-commit") ?? "");
  const clineShas = clineEls.map((card) => card.getAttribute("data-sha") ?? "");
  const fileTreeMeta = fileEls.map((el) => {
    const path = el.getAttribute("data-path") || "";
    const parts = path.split("/");
    return {
      dirs: parts.slice(0, -1).filter(Boolean),
      name: parts[parts.length - 1] ?? "",
      pathLower: path.toLowerCase(),
      commitShas: (el.getAttribute("data-commits") || "").split(" ").filter(Boolean),
      status: el.getAttribute("data-status") || "modified",
      statusCode: el.getAttribute("data-status-code") || "M",
      statusLabel: el.getAttribute("data-status-label") || "Modified file",
    };
  });
  let ownedRows: HTMLElement[] = [];
  let treeFileLeaves: Array<{ readonly leaf: HTMLElement; readonly targetId: string | null }> = [];
  const treeBody = root.querySelector<HTMLElement>(".tree-body");
  const filterInput = root.querySelector<HTMLInputElement>(".search input");
  const foldAll = root.querySelector<HTMLElement>(".foldall");
  let focus: CommitFocus | null = null;
  let filterText = "";

  const mainScroller = root.querySelector<HTMLElement>(".main") ?? root;

  if (treeBody)
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
    const selected = focus === null ? null : focus.shas;
    planFileVisibility(fileTreeMeta, { filterText, activeShas: selected }).forEach((isHidden, index) => {
      const el = fileEls[index];
      if (el) el.hidden = isHidden;
    });
    syncBeads(selected);
    syncOwned(selected);
    buildTree();
  }

  if (filterInput)
    listen(filterInput, "input", () => {
      filterText = filterInput.value.trim().toLowerCase();
      applyFilter();
    });

  if (foldAll)
    listen(foldAll, "click", () => {
      const anyOpen = fileEls.some((el) => el.open);
      fileEls.forEach((el) => {
        el.open = !anyOpen;
      });
    });

  // The context toggle drives `.copy-ctx` on this view's root; the copy button reads that
  // class at click time to decide whether to prepend the path/lines header.
  const ctxToggle = root.querySelector<HTMLElement>(".ctx-toggle");
  if (ctxToggle)
    listen(ctxToggle, "click", () => {
      const on = root.classList.toggle("copy-ctx");
      ctxToggle.setAttribute("aria-pressed", on ? "true" : "false");
      ctxToggle.classList.toggle("active", on);
    });

  function bindHorizontalWheel(scroller: HTMLElement): void {
    listen(
      scroller,
      "wheel",
      (ev) => {
        if (!(ev instanceof WheelEvent)) return;
        const next = computeWheelScroll(scroller, ev);
        if (next === null) return;
        ev.stopPropagation();
        ev.preventDefault();
        scroller.scrollLeft = next;
      },
      { passive: false },
    );
  }
  root.querySelectorAll<HTMLElement>(".diff").forEach(bindHorizontalWheel);

  interface TreeFile {
    readonly name: string;
    readonly el: HTMLDetailsElement;
    readonly status: string;
    readonly statusCode: string;
    readonly statusLabel: string;
  }

  // The tree follows DOM order, never re-sorts: the server already tree-sorted the files,
  // and the sidebar must match the center pane exactly. A Map keyed by path segment carries
  // that insertion order, and keeps a directory named `constructor` or `__proto__` from
  // resolving to an Object.prototype member.
  interface TreeNode {
    readonly dirs: Map<string, TreeNode>;
    readonly files: TreeFile[];
  }

  function buildTree(): void {
    if (!treeBody) return;
    const treeRoot: TreeNode = { dirs: new Map(), files: [] };
    fileEls.forEach((el, index) => {
      const meta = fileTreeMeta[index];
      if (el.hidden || !meta) return;
      let node = treeRoot;
      for (const dir of meta.dirs) {
        let child = node.dirs.get(dir);
        if (!child) {
          child = { dirs: new Map(), files: [] };
          node.dirs.set(dir, child);
        }
        node = child;
      }
      node.files.push({
        name: meta.name,
        el,
        status: meta.status,
        statusCode: meta.statusCode,
        statusLabel: meta.statusLabel,
      });
    });
    treeBody.innerHTML = "";
    treeBody.appendChild(renderNode(treeRoot));
    treeFileLeaves = Array.from(treeBody.querySelectorAll<HTMLElement>(".tfile")).map((leaf) => ({
      leaf,
      targetId: leaf.getAttribute("data-target"),
    }));
  }

  function renderNode(node: TreeNode): HTMLUListElement {
    const ul = document.createElement("ul");
    node.dirs.forEach((child, name) => {
      const li = document.createElement("li");
      li.className = "tnode tdir open";
      const label = document.createElement("div");
      label.className = "tlabel";
      const caret = document.createElement("span");
      caret.className = "tcaret";
      const dirName = document.createElement("span");
      dirName.className = "tname";
      dirName.textContent = name;
      label.appendChild(caret);
      label.appendChild(dirName);
      li.appendChild(label);
      li.appendChild(renderNode(child));
      ul.appendChild(li);
    });
    node.files.forEach((f) => {
      const li = buildFileLeaf(document, f);
      ul.appendChild(li);
    });
    return ul;
  }

  function markCurrent(el: HTMLElement): void {
    treeFileLeaves.forEach(({ leaf, targetId }) => {
      leaf.classList.toggle("cur", targetId === el.id);
    });
  }

  function syncBeads(selected: ReadonlySet<string> | null): void {
    clineEls.forEach((c, index) => {
      c.classList.toggle("active", selected !== null && selected.has(clineShas[index] ?? ""));
    });
  }

  // Dim the diff (.commit-focus) and lift only the rows whose data-commit is selected.
  function syncOwned(selected: ReadonlySet<string> | null): void {
    ownedRows.forEach((r) => {
      r.classList.remove("owned");
    });
    ownedRows = [];
    if (selected) {
      dlCommits.forEach((commit, index) => {
        if (!selected.has(commit)) return;
        const row = dlEls[index];
        if (!row) return;
        row.classList.add("owned");
        ownedRows.push(row);
      });
    }
    root.classList.toggle("commit-focus", selected !== null);
  }

  clineEls.forEach((c) => {
    const sha = c.getAttribute("data-sha") ?? "";
    listen(c, "click", () => {
      focus = resolveActiveSet(sha, c.getAttribute("data-members") || "", focus === null ? null : focus.sha);
      applyFilter();
    });
    listen(c, "keydown", (event) => {
      if (!(event instanceof KeyboardEvent)) return;
      if (event.target instanceof Element && isShaTarget(event.target)) return;
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        c.click();
      }
    });

    const hashCopy = c.querySelector<HTMLElement>(".sha");
    if (hashCopy)
      listen(hashCopy, "click", (event) => {
        event.stopPropagation();
        void copyText(sha);
        c.classList.add("copied");
        later(() => {
          c.classList.remove("copied");
        }, 900);
      });

    const popId = c.getAttribute("data-pop");
    if (!popId) return;
    const cssEscaped = window.CSS && CSS.escape ? CSS.escape(popId) : popId;
    const pop = root.querySelector<HTMLElement>(`#${cssEscaped}`);
    if (!pop) return;
    const popover = pop;
    let hideTimer: ReturnType<typeof setTimeout> | undefined;
    function show(): void {
      if (hideTimer !== undefined) cancel(hideTimer);
      const r = c.getBoundingClientRect();
      let left = r.left - 338;
      if (left < 8) left = r.right + 6;
      popover.style.left = `${left}px`;
      popover.style.top = `${Math.min(r.top, innerHeight - 210)}px`;
      popover.showPopover?.();
    }
    function hide(): void {
      hideTimer = later(() => {
        hideTimer = undefined;
        popover.hidePopover?.();
      }, 140);
    }
    listen(c, "mouseenter", show);
    listen(c, "mouseleave", hide);
    listen(popover, "mouseenter", () => {
      if (hideTimer === undefined) return;
      cancel(hideTimer);
      hideTimer = undefined;
    });
    listen(popover, "mouseleave", hide);
  });

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

  root.querySelectorAll<HTMLElement>(".ln-more").forEach((button) => {
    listen(button, "click", () => toggleLongLine(button));
  });

  buildTree();
  return destroy;
}
