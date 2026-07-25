import { copyText } from "../core/clipboard";
import { keyboardCommand } from "../core/keyboard";
import { scrollLandOn } from "../core/scroll";
import { showToast } from "../core/toast";
import { isShaTarget, resolveActiveSet } from "./commit-focus";
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
  if (!file) return;
  const layout = file.closest(".layout");
  if (layout && !layout.classList.contains("copy-ctx")) return;
  // Pane visibility is CSS-driven, so read rows from the pane the selection sits in. Split
  // panes carry no .dl-add rows; their selections fall through to the native copy.
  const diffBlock = el?.closest ? el.closest(".diff") : null;
  const rows = diffBlock ? Array.from(diffBlock.querySelectorAll(".dl-add, .dl-ctx")) : [];
  const out: string[] = [];
  let first: number | null = null;
  let last: number | null = null;
  rows.forEach((row) => {
    if (!sel.containsNode(row, true)) return;
    const code = row.querySelector("code");
    if (!code) return;
    // .code-text keeps long-line expander chrome out of the copied text.
    const text = (code.querySelector(".code-text") ?? code).textContent ?? "";
    out.push(text.length && (text[0] === "+" || text[0] === " ") ? text.slice(1) : text);
    const lns = row.querySelectorAll(".ln");
    const n = lns.length > 1 ? parseInt(lns[1]?.textContent ?? "", 10) : NaN;
    if (!Number.isNaN(n)) {
      if (first === null) first = n;
      last = n;
    }
  });
  if (!out.length || !e.clipboardData) return;
  const leader = file.getAttribute("data-comment") || "//";
  const path = file.getAttribute("data-path") || "";
  const range = first === last ? `${first}` : `${first}..${last}`;
  const header = `${leader} * ${path}` + (first !== null ? `, lines: ${range}` : "");
  e.clipboardData.setData("text/plain", `${header}\n${out.join("\n")}`);
  e.preventDefault();
  showToast(first !== null ? `Copied with context · lines ${range}` : "Copied with context");
}

/**
 * Opens `target`, then schedules `land` in the next animation frame so a collapsed
 * content-visibility giant materializes before the landing correction loop runs.
 */
export function navigateToFile(
  target: HTMLDetailsElement,
  scroller: HTMLElement,
  opts: {
    raf?: (cb: FrameRequestCallback) => number;
    land?: (t: HTMLDetailsElement, s: HTMLElement) => void;
    stickyTop?: number;
  },
): void {
  const raf = opts.raf ?? ((cb) => requestAnimationFrame(cb));
  const land = opts.land ?? ((t, s) => scrollLandOn(t, s, { stickyTop: opts.stickyTop ?? 0 }));
  target.open = true;
  raf(() => land(target, scroller));
}

export function enhanceLayout(root: HTMLElement): () => void {
  const cleanups: Array<() => void> = [];
  const timers = new Set<ReturnType<typeof setTimeout>>();
  const listen = (
    target: EventTarget,
    type: string,
    listener: EventListener,
    options?: AddEventListenerOptions,
  ): void => {
    target.addEventListener(type, listener, options);
    cleanups.push(() => target.removeEventListener(type, listener, options));
  };
  const later = (callback: () => void, delay: number): ReturnType<typeof setTimeout> => {
    const timer = setTimeout(() => {
      timers.delete(timer);
      callback();
    }, delay);
    timers.add(timer);
    return timer;
  };
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
  const fileTreeMeta = fileEls.map((el) => ({
    parts: (el.getAttribute("data-path") || "").split("/"),
    status: el.getAttribute("data-status") || "modified",
    statusCode: el.getAttribute("data-status-code") || "M",
    statusLabel: el.getAttribute("data-status-label") || "Modified file",
  }));
  let ownedRows: HTMLElement[] = [];
  let treeFileLeaves: Array<{ readonly leaf: HTMLElement; readonly targetId: string | null }> = [];
  const treeBody = root.querySelector<HTMLElement>(".tree-body");
  const filterInput = root.querySelector<HTMLInputElement>(".search input");
  const foldAll = root.querySelector<HTMLElement>(".foldall");
  let activeSha: string | null = null;
  let activeSet: string[] | null = null;
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
    navigateToFile(t, mainScroller, { stickyTop });
    t.classList.add("flash");
    later(() => {
      t.classList.remove("flash");
    }, 1200);
    markCurrent(t);
  }

  function applyFilter(): void {
    const selected = activeSet === null ? null : new Set(activeSet);
    const hidden = planFileVisibility(fileEls, { filterText, activeShas: activeSet });
    fileEls.forEach((el, index) => {
      el.hidden = hidden[index] ?? false;
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

  function bindHorizontalWheel(scroller: Element): void {
    listen(
      scroller,
      "wheel",
      (ev) => {
        if (!(ev instanceof WheelEvent) || !(scroller instanceof HTMLElement)) return;
        const next = computeWheelScroll(scroller, ev);
        if (next === null) return;
        ev.stopPropagation();
        ev.preventDefault();
        scroller.scrollLeft = next;
      },
      { passive: false },
    );
  }
  root.querySelectorAll(".diff").forEach(bindHorizontalWheel);

  // The tree follows DOM order, never re-sorts: the server already tree-sorted the files,
  // and the sidebar must match the center pane exactly.
  interface TreeNode {
    dirs: Record<string, TreeNode>;
    dirOrder: string[];
    files: Array<{
      name: string;
      el: HTMLDetailsElement;
      status: string;
      statusCode: string;
      statusLabel: string;
    }>;
  }

  function buildTree(): void {
    if (!treeBody) return;
    const treeRoot: TreeNode = { dirs: {}, dirOrder: [], files: [] };
    fileEls.forEach((el, index) => {
      const meta = fileTreeMeta[index];
      if (el.hidden || !meta) return;
      const parts = meta.parts;
      let node = treeRoot;
      for (let i = 0; i < parts.length - 1; i++) {
        const part = parts[i];
        if (!part) continue;
        if (!node.dirs[part]) {
          node.dirs[part] = { dirs: {}, dirOrder: [], files: [] };
          node.dirOrder.push(part);
        }
        const child = node.dirs[part];
        if (child) node = child;
      }
      node.files.push({
        name: parts[parts.length - 1] ?? "",
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
    node.dirOrder.forEach((name) => {
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
      const child = node.dirs[name];
      if (child) li.appendChild(renderNode(child));
      ul.appendChild(li);
    });
    node.files.forEach((f) => {
      const li = buildFileLeaf(document, f);
      ul.appendChild(li);
    });
    return ul;
  }

  function markCurrent(el: HTMLElement | null): void {
    if (!el) return;
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
      dlEls.forEach((r, index) => {
        if (selected.has(dlCommits[index] ?? "")) {
          r.classList.add("owned");
          ownedRows.push(r);
        }
      });
    }
    root.classList.toggle("commit-focus", selected !== null);
  }

  clineEls.forEach((c) => {
    const sha = c.getAttribute("data-sha") ?? "";
    listen(c, "click", () => {
      ({ sha: activeSha, set: activeSet } = resolveActiveSet(sha, c.getAttribute("data-members") || "", activeSha));
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
      if (hideTimer !== undefined) {
        clearTimeout(hideTimer);
        timers.delete(hideTimer);
      }
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
      clearTimeout(hideTimer);
      timers.delete(hideTimer);
      hideTimer = undefined;
    });
    listen(popover, "mouseleave", hide);
  });

  let curFile = -1;

  function focusFile(i: number): void {
    const visible = fileEls.filter((el) => !el.hidden);
    if (!visible.length) return;
    curFile = Math.max(0, Math.min(i, visible.length - 1));
    const t = visible[curFile];
    if (!t) return;
    openAndScrollTo(t);
  }

  // Listen on document (a div gets no keydown without focus) but ignore events while this
  // layout's tabbed panel is hidden, so each panel stays independently driven.
  listen(document, "keydown", (event) => {
    if (!(event instanceof KeyboardEvent)) return;
    const panel = root.closest<HTMLElement>(".panel");
    if (panel && panel.hidden) return;
    const command = keyboardCommand(event);
    if (command === "blur-input") {
      if (event.target instanceof HTMLElement) event.target.blur();
      return;
    }
    if (command === "fold-all") {
      event.preventDefault();
      foldAll?.click();
    } else if (command === "focus-filter") {
      event.preventDefault();
      filterInput?.focus();
    } else if (command === "next-file") {
      event.preventDefault();
      focusFile(curFile + 1);
    } else if (command === "previous-file") {
      event.preventDefault();
      focusFile(curFile - 1);
    }
  });

  root.querySelectorAll<HTMLElement>(".ln-more").forEach((button) => {
    listen(button, "click", () => toggleLongLine(button));
  });

  buildTree();
  return () => {
    cleanups.reverse().forEach((cleanup) => cleanup());
    timers.forEach(clearTimeout);
    timers.clear();
  };
}
