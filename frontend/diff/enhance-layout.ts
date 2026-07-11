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
  toastTimer = setTimeout(() => {
    toastEl?.classList.remove("show");
  }, 1600);
}

// ! Intercept native copy (ctrl+c / context menu) of a selection inside a diff: rebuild the
// ! selected add/context rows as clean source (markers stripped, gutter is user-select:none)
// ! and prepend the same commented "path, lines" header the copy button uses, so snippets
// ! paste elsewhere already labelled. Selection granularity is whole lines. Bails to the
// ! native copy for empty selections, selections outside a single diff file, selections that
// ! touch no code rows, or when that view's context toggle is off.
export function handleDocumentCopy(e: ClipboardEvent): void {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || !sel.rangeCount || !sel.containsNode) return;
  const node = sel.getRangeAt(0).commonAncestorContainer;
  const el = node instanceof Element ? node : node.parentElement;
  const file = el?.closest ? el.closest("details.file") : null;
  if (!file) return;
  const layout = file.closest(".layout");
  if (layout && !layout.classList.contains("copy-ctx")) return;
  // ! Read rows from the pane the selection sits in (visibility is CSS-driven now, so
  // ! `:not([hidden])` no longer marks the live one). Split panes carry no .dl-add rows, so a
  // ! selection there yields nothing and we fall through to the browser's native copy.
  const diffBlock = el?.closest ? el.closest(".diff") : null;
  const rows = diffBlock ? Array.from(diffBlock.querySelectorAll(".dl-add, .dl-ctx")) : [];
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

// * Opens `target` then schedules `land` in the next animation frame so a collapsed
// * content-visibility giant is materialized before scrollLandOn runs its correction loop.
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
  const listen = (target: EventTarget, type: string, listener: EventListener, options?: AddEventListenerOptions): void => {
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
  // ! Scope every view to its own .layout root: the tabbed (diff subrepos) view inlines one
  // ! .layout per panel in a single document, so document.querySelector would only ever wire
  // ! the first panel. Querying within `root` keeps each tab independently interactive.
  const fileEls = Array.from(root.querySelectorAll<HTMLDetailsElement>("details.file"));
  const clineEls = Array.from(root.querySelectorAll<HTMLElement>(".cline[data-sha]"));
  const dlEls = Array.from(
    root.querySelectorAll<HTMLElement>(".dl-add[data-commit],.dl-del[data-commit],.sp[data-commit]"),
  );
  let ownedRows: HTMLElement[] = [];
  const treeBody = root.querySelector<HTMLElement>(".tree-body");
  const filterInput = root.querySelector<HTMLInputElement>(".search input");
  const foldAll = root.querySelector<HTMLElement>(".foldall");
  const viewToggle = root.querySelector<HTMLElement>(".view-toggle");
  const layoutToggle = root.querySelector<HTMLElement>(".layout-toggle");
  const docEl = document.documentElement;
  let activeSha: string | null = null;
  let activeSet: string[] | null = null;
  let filterText = "";

  // Compute the main scroller and sticky offset once
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
    later(() => {
      t.classList.remove("flash");
    }, 1200);
    markCurrent(t);
  }

  // ---- commit filter + name filter: a file shows only if it survives both ----
  function applyFilter(): void {
    fileEls.forEach((el) => {
      const selected = activeSet;
      const byCommit = selected !== null && !shasOf(el).some((sha) => selected.includes(sha));
      el.hidden = byCommit || !matchesFilter(el);
    });
    syncBeads();
    syncOwned();
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

  // ! Layout (split vs unified) and full-file mode are global view preferences on the <html>
  // ! data-attrs (like the theme): CSS reveals the right pane, and the choice is shared across
  // ! tabbed panels. Layout persists per-device; full-file resets each artifact.
  function syncToggles(): void {
    const full = docEl.dataset["diffFull"] === "on";
    const split = docEl.dataset["diffLayout"] === "split";
    document.querySelectorAll<HTMLElement>(".view-toggle").forEach((b) => {
      b.setAttribute("aria-pressed", full ? "true" : "false");
      b.classList.toggle("active", full);
    });
    document.querySelectorAll<HTMLElement>(".layout-toggle").forEach((b) => {
      b.setAttribute("aria-pressed", split ? "true" : "false");
      b.classList.toggle("active", split);
    });
  }

  if (viewToggle)
    listen(viewToggle, "click", () => {
      if (docEl.dataset["diffFull"] === "on") delete docEl.dataset["diffFull"];
      else docEl.dataset["diffFull"] = "on";
      syncToggles();
    });

  if (layoutToggle)
    listen(layoutToggle, "click", () => {
      const split = docEl.dataset["diffLayout"] !== "split"; // currently unified -> switch to split
      if (split) docEl.dataset["diffLayout"] = "split";
      else delete docEl.dataset["diffLayout"];
      try {
        localStorage.setItem("gtl-diff-layout", split ? "split" : "unified");
      } catch {
        /* storage unavailable (private mode / file://) — the in-page toggle still works */
      }
      syncToggles();
    });

  // ! Context toggle drives the `.copy-ctx` class on this view's root; the copy-button
  // ! reads that class at click time to decide whether to prepend the path/lines header.
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
        name: parts.at(-1) ?? "",
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
    ownedRows.forEach((r) => {
      r.classList.remove("owned");
    });
    ownedRows = [];
    const selected = activeSet;
    if (selected) {
      dlEls.forEach((r) => {
        if (selected.includes(r.getAttribute("data-commit") ?? "")) {
          r.classList.add("owned");
          ownedRows.push(r);
        }
      });
    }
    root.classList.toggle("commit-focus", !!activeSet);
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
  syncToggles();
  return () => {
    cleanups.reverse().forEach((cleanup) => cleanup());
    timers.forEach(clearTimeout);
    timers.clear();
  };
}
