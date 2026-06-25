import { copyText } from "../core/clipboard";
import { keyboardCommand } from "../core/keyboard";
import { scrollLandOn } from "../core/scroll";
import { isShaTarget, resolveActiveSet } from "./commit-focus";
import { buildFileLeaf } from "./file-tree";
import { toggleLongLine } from "./long-lines";
import { computeWheelScroll } from "./wheel";

// ! Singleton fade toast appended once to <body>; re-triggering restarts the timer.
let toastEl: HTMLElement | null = null;
let toastTimer: ReturnType<typeof setTimeout> | null = null;

function showToast(msg: string): void {
  if (!toastEl) {
    toastEl = document.createElement("div");
    toastEl.className = "gtl-toast";
    document.body.appendChild(toastEl);
  }
  toastEl.textContent = msg;
  void toastEl.offsetWidth; // reflow so the transition re-runs on rapid copies
  toastEl.classList.add("show");
  if (toastTimer !== null) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { toastEl?.classList.remove("show"); }, 1600);
}

// ! Intercept native copy (ctrl+c / context menu) of a selection inside a diff: rebuild the
// ! selected add/context rows as clean source (markers stripped, gutter is user-select:none)
// ! and prepend the same commented "path, lines" header the copy button uses, so snippets
// ! paste elsewhere already labelled. Selection granularity is whole lines. Bails to the
// ! native copy for empty selections, selections outside a single diff file, selections that
// ! touch no code rows, or when that view's context toggle is off.
document.addEventListener("copy", (e: ClipboardEvent) => {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || !sel.rangeCount || !sel.containsNode) return;
  const node = sel.getRangeAt(0).commonAncestorContainer;
  const el = node.nodeType === 1 ? (node as Element) : (node as Node).parentElement;
  const file = el?.closest ? el.closest("details.file") : null;
  if (!file) return;
  const layout = file.closest(".layout");
  if (layout && !layout.classList.contains("copy-ctx")) return;
  const rows = file.querySelectorAll(".diff:not([hidden]) .dl-add, .diff:not([hidden]) .dl-ctx");
  const out: string[] = [];
  let first: number | null = null;
  let last: number | null = null;
  rows.forEach((row) => {
    if (!sel.containsNode(row, true)) return;
    const code = row.querySelector("code");
    if (!code) return;
    // ! prefer .code-text so a future dl-long selection copies cleanly (mirrors extractCopyText)
    const text = (code.querySelector(".code-text") ?? code).textContent ?? "";
    out.push(text.length && (text[0] === "+" || text[0] === " ") ? text.slice(1) : text);
    const lns = row.querySelectorAll(".ln");
    const n = lns.length > 1 ? parseInt(lns[1]?.textContent ?? "", 10) : NaN;
    if (!Number.isNaN(n)) { if (first === null) first = n; last = n; }
  });
  if (!out.length || !e.clipboardData) return;
  const leader = file.getAttribute("data-comment") || "//";
  const path = file.getAttribute("data-path") || "";
  const range = first === last ? `${first}` : `${first}..${last}`;
  const header = `${leader} * ${path}` + (first !== null ? `, lines: ${range}` : "");
  e.clipboardData.setData("text/plain", `${header}\n${out.join("\n")}`);
  e.preventDefault();
  showToast(first !== null ? `Copied with context · lines ${range}` : "Copied with context");
});

// * Opens `target` then schedules `land` in the next animation frame so a collapsed
// * content-visibility giant is materialized before scrollLandOn runs its correction loop.
export function navigateToFile(
  target: HTMLDetailsElement,
  scroller: HTMLElement,
  opts: { raf?: (cb: FrameRequestCallback) => number; land?: (t: HTMLDetailsElement, s: HTMLElement) => void; stickyTop?: number },
): void {
  const raf = opts.raf ?? ((cb) => requestAnimationFrame(cb));
  const land = opts.land ?? ((t, s) => scrollLandOn(t, s, { stickyTop: opts.stickyTop ?? 0 }));
  target.open = true;
  raf(() => land(target, scroller));
}

export function enhanceLayout(root: HTMLElement): void {
  // ! Scope every view to its own .layout root: the tabbed (diff subrepos) view inlines one
  // ! .layout per panel in a single document, so document.querySelector would only ever wire
  // ! the first panel. Querying within `root` keeps each tab independently interactive.
  const fileEls = Array.from(root.querySelectorAll<HTMLDetailsElement>("details.file"));
  const clineEls = Array.from(root.querySelectorAll<HTMLElement>(".cline[data-sha]"));
  const dlEls = Array.from(root.querySelectorAll<HTMLElement>(".dl-add[data-commit],.dl-del[data-commit]"));
  let ownedRows: HTMLElement[] = [];
  const treeBody = root.querySelector<HTMLElement>(".tree-body");
  const filterInput = root.querySelector<HTMLInputElement>(".search input");
  const foldAll = root.querySelector<HTMLElement>(".foldall");
  const viewToggle = root.querySelector<HTMLElement>(".view-toggle");
  let activeSha: string | null = null;
  let activeSet: string[] | null = null;
  let filterText = "";

  // Compute the main scroller and sticky offset once
  const mainScroller = root.querySelector<HTMLElement>(".main") ?? root;

  function esc(s: string): string {
    return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }
  function shasOf(el: Element): string[] {
    return (el.getAttribute("data-commits") || "").split(" ").filter(Boolean);
  }
  function matchesFilter(el: Element): boolean {
    return !filterText || (el.getAttribute("data-path") || "").toLowerCase().indexOf(filterText) !== -1;
  }

  // ---- single helper used by both tree-leaf click and focusFile keyboard nav ----
  function openAndScrollTo(t: HTMLDetailsElement): void {
    const summaryEl = t.querySelector<HTMLElement>("summary");
    const stickyTop = summaryEl ? summaryEl.offsetHeight : 0;
    navigateToFile(t, mainScroller, { stickyTop });
    t.classList.add("flash");
    setTimeout(() => { t.classList.remove("flash"); }, 1200);
    markCurrent(t);
  }

  // ---- commit filter + name filter: a file shows only if it survives both ----
  function applyFilter(): void {
    fileEls.forEach((el) => {
      const byCommit = !!activeSet && !shasOf(el).some((s) => activeSet!.indexOf(s) !== -1);
      el.hidden = byCommit || !matchesFilter(el);
    });
    syncBeads();
    syncOwned();
    buildTree();
  }

  if (filterInput) filterInput.addEventListener("input", () => {
    filterText = filterInput.value.trim().toLowerCase();
    applyFilter();
  });

  if (foldAll) foldAll.addEventListener("click", () => {
    const anyOpen = fileEls.some((el) => el.open);
    fileEls.forEach((el) => { el.open = !anyOpen; });
  });

  function setFullMode(on: boolean): void {
    fileEls.forEach((el) => {
      const compact = el.querySelector<HTMLElement>(".diff-compact");
      const full = el.querySelector<HTMLElement>(".diff-full");
      if (!compact) return;
      if (!full) {
        compact.hidden = false;
        return;
      }
      compact.hidden = on;
      full.hidden = !on;
    });
    if (viewToggle) {
      viewToggle.setAttribute("aria-pressed", on ? "true" : "false");
      viewToggle.classList.toggle("active", on);
    }
  }

  if (viewToggle) viewToggle.addEventListener("click", () => {
    setFullMode(viewToggle.getAttribute("aria-pressed") !== "true");
  });

  // ! Context toggle drives the `.copy-ctx` class on this view's root; the copy-button
  // ! reads that class at click time to decide whether to prepend the path/lines header.
  const ctxToggle = root.querySelector<HTMLElement>(".ctx-toggle");
  if (ctxToggle) ctxToggle.addEventListener("click", () => {
    const on = root.classList.toggle("copy-ctx");
    ctxToggle.setAttribute("aria-pressed", on ? "true" : "false");
    ctxToggle.classList.toggle("active", on);
  });

  function bindHorizontalWheel(scroller: Element): void {
    scroller.addEventListener("wheel", (ev) => {
      const e = ev as WheelEvent;
      const el = scroller as HTMLElement;
      const next = computeWheelScroll(el, e);
      if (next === null) return;
      e.stopPropagation();
      e.preventDefault();
      el.scrollLeft = next;
    }, { passive: false });
  }
  root.querySelectorAll(".diff").forEach(bindHorizontalWheel);

  // ---- file tree: fold visible files into a nested, collapsible tree.
  // ! Follow DOM/server order (already tree-sorted in render) — DON'T re-sort, so the
  // ! sidebar matches the center pane exactly. ----
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
    fileEls.forEach((el) => {
      if (el.hidden) return;
      const parts = (el.getAttribute("data-path") || "").split("/");
      let node = treeRoot;
      for (let i = 0; i < parts.length - 1; i++) {
        const part = parts[i] as string;
        if (!node.dirs[part]) {
          node.dirs[part] = { dirs: {}, dirOrder: [], files: [] };
          node.dirOrder.push(part);
        }
        node = node.dirs[part] as TreeNode;
      }
      node.files.push({
        name: parts[parts.length - 1] as string,
        el,
        status: el.getAttribute("data-status") || "modified",
        statusCode: el.getAttribute("data-status-code") || "M",
        statusLabel: el.getAttribute("data-status-label") || "Modified file",
      });
    });
    treeBody.innerHTML = "";
    treeBody.appendChild(renderNode(treeRoot));
  }

  function renderNode(node: TreeNode): HTMLUListElement {
    const ul = document.createElement("ul");
    node.dirOrder.forEach((name) => {
      const li = document.createElement("li");
      li.className = "tnode tdir open";
      const label = document.createElement("div");
      label.className = "tlabel";
      label.innerHTML = `<span class="tcaret"></span><span class="tname">${esc(name)}</span>`;
      label.addEventListener("click", () => { li.classList.toggle("open"); });
      li.appendChild(label);
      li.appendChild(renderNode(node.dirs[name] as TreeNode));
      ul.appendChild(li);
    });
    node.files.forEach((f) => {
      const li = buildFileLeaf(document, f);
      const label = li.querySelector<HTMLElement>(".tlabel")!;
      label.addEventListener("click", () => { openAndScrollTo(f.el); });
      ul.appendChild(li);
    });
    return ul;
  }

  // ! mark the active file's sidebar leaf so the tree shows where you are (j/k + tree-click).
  function markCurrent(el: HTMLElement | null): void {
    if (!treeBody || !el) return;
    treeBody.querySelectorAll<HTMLElement>(".tfile").forEach((li) => {
      li.classList.toggle("cur", li.getAttribute("data-target") === el.id);
    });
  }

  // ---- commit shelf cards: card click filters files; hash tag click copies the hash;
  // hover a card WITH notes shows its native popover (top-layer escapes the shelf scroll clip). ----
  function syncBeads(): void {
    clineEls.forEach((c) => {
      const on = !!activeSet && activeSet.indexOf(c.getAttribute("data-sha") ?? "") !== -1;
      c.classList.toggle("active", on);
    });
  }

  // ! Focus the selected commit's own rows: dim the diff (.commit-focus) and lift only the
  // ! rows whose data-commit is in activeSet. Touches just the matching rows, not a re-scan.
  function syncOwned(): void {
    ownedRows.forEach((r) => { r.classList.remove("owned"); });
    ownedRows = [];
    if (activeSet) {
      dlEls.forEach((r) => {
        if (activeSet!.indexOf(r.getAttribute("data-commit") ?? "") !== -1) {
          r.classList.add("owned");
          ownedRows.push(r);
        }
      });
    }
    root.classList.toggle("commit-focus", !!activeSet);
  }

  clineEls.forEach((c) => {
    const sha = c.getAttribute("data-sha") ?? "";
    c.addEventListener("click", () => {
      ({ sha: activeSha, set: activeSet } = resolveActiveSet(sha, c.getAttribute("data-members") || "", activeSha));
      applyFilter();
    });
    c.addEventListener("keydown", (ev) => {
      const e = ev as KeyboardEvent;
      if (isShaTarget(e.target as Element | null)) return;
      if (e.key === "Enter" || e.key === " ") { e.preventDefault(); c.click(); }
    });

    const hashCopy = c.querySelector<HTMLElement>(".sha");
    if (hashCopy) hashCopy.addEventListener("click", (e) => {
      e.stopPropagation();
      copyText(sha);
      c.classList.add("copied");
      setTimeout(() => { c.classList.remove("copied"); }, 900);
    });

    const popId = c.getAttribute("data-pop");
    if (!popId) return;
    const cssEscaped = window.CSS && CSS.escape ? CSS.escape(popId) : popId;
    const pop = root.querySelector<HTMLElement>(`#${cssEscaped}`);
    if (!pop) return;
    let t: ReturnType<typeof setTimeout>;
    function show(): void {
      clearTimeout(t);
      const r = c.getBoundingClientRect();
      let left = r.left - 338;
      if (left < 8) left = r.right + 6;
      pop!.style.left = `${left}px`;
      pop!.style.top = `${Math.min(r.top, innerHeight - 210)}px`;
      if ("showPopover" in pop!) (pop as HTMLElement & { showPopover(): void }).showPopover();
    }
    function hide(): void {
      t = setTimeout(() => {
        if ("hidePopover" in pop!) (pop as HTMLElement & { hidePopover(): void }).hidePopover();
      }, 140);
    }
    c.addEventListener("mouseenter", show);
    c.addEventListener("mouseleave", hide);
    pop.addEventListener("mouseenter", () => { clearTimeout(t); });
    pop.addEventListener("mouseleave", hide);
  });

  // ---- keyboard (scoped): / focus filter, j/k next/prev file, alt+shift+c fold all ----
  let curFile = -1;

  function focusFile(i: number): void {
    const visible = fileEls.filter((el) => !el.hidden);
    if (!visible.length) return;
    curFile = Math.max(0, Math.min(i, visible.length - 1));
    const t = visible[curFile];
    if (!t) return;
    openAndScrollTo(t);
  }

  // ! Listen on document (a div gets no keydown without focus) but ignore events while this
  // ! layout's tabbed panel is hidden, so each panel stays independently driven.
  document.addEventListener("keydown", (ev) => {
    const e = ev as KeyboardEvent;
    const panel = root.closest<HTMLElement>(".panel");
    if (panel && panel.hidden) return;
    const command = keyboardCommand(e);
    if (command === "blur-input") {
      (e.target as HTMLElement).blur();
      return;
    }
    if (command === "fold-all") {
      e.preventDefault();
      foldAll?.click();
    } else if (command === "focus-filter") {
      e.preventDefault();
      filterInput?.focus();
    } else if (command === "next-file") {
      e.preventDefault();
      focusFile(curFile + 1);
    } else if (command === "previous-file") {
      e.preventDefault();
      focusFile(curFile - 1);
    }
  });

  root.querySelectorAll(".ln-more").forEach((b) => b.addEventListener("click", () => toggleLongLine(b)));

  buildTree();
  setFullMode(false);
}
