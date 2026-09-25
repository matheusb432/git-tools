(() => {
  "use strict";

  const root = document.querySelector("[data-gtl-artifact-ready='true']");
  if (root === null) return;
  // The renderer localizes these labels into the artifact's language.
  const labels = root.dataset;

  const feedbackTimers = new WeakMap();
  const copyContextFeedbackTimers = new WeakMap();
  const flashTimers = new WeakMap();
  const hoverPopoverTimers = new WeakMap();
  const scrollAreas = new WeakMap();
  let scrollbarDrag = null;

  function scrollMetrics(area, axis) {
    const viewport = area.viewport;
    return axis === "horizontal"
      ? [viewport.clientWidth, viewport.scrollWidth, viewport.scrollLeft]
      : [viewport.clientHeight, viewport.scrollHeight, viewport.scrollTop];
  }

  function thumbGeometry(viewport, content, position) {
    const maximum = Math.max(content - viewport, 0);
    const size = content > 0 ? Math.min(viewport, Math.max(24, viewport * viewport / content)) : viewport;
    const travel = viewport - size;
    return { size, offset: maximum > 0 ? Math.max(0, Math.min(maximum, position)) / maximum * travel : 0, maximum, travel };
  }

  function scrollStyle(element, name, value) {
    if (element.style.getPropertyValue(name) !== value) element.style.setProperty(name, value);
  }

  function scrollAttribute(element, name, value) {
    if (element.getAttribute(name) !== value) element.setAttribute(name, value);
  }

  function refreshScrollbars(area) {
    const { viewport, layer, rails } = area;
    if (viewport.clientWidth === 0 || viewport.clientHeight === 0) return;
    scrollStyle(layer, "--scroll-width", `${viewport.clientWidth}px`);
    scrollStyle(layer, "--scroll-height", `${viewport.clientHeight}px`);
    for (const rail of rails) {
      const axis = rail.dataset.scrollAxis;
      const [size, content, position] = scrollMetrics(area, axis);
      const geometry = thumbGeometry(size, content, position);
      if (axis === "horizontal") scrollAttribute(viewport, "data-scroll-overflow-x", String(content > size));
      scrollAttribute(rail, "aria-hidden", String(content <= size));
      scrollAttribute(rail, "tabindex", content > size ? "0" : "-1");
      scrollAttribute(rail, "aria-valuemax", String(geometry.maximum));
      scrollAttribute(rail, "aria-valuenow", String(position));
      const thumb = rail.firstElementChild;
      scrollStyle(thumb, axis === "horizontal" ? "width" : "height", `${geometry.size}px`);
      scrollStyle(thumb, "transform", `translate${axis === "horizontal" ? "X" : "Y"}(${geometry.offset}px)`);
    }
  }

  function measureScrollbars(area) {
    const style = getComputedStyle(area.viewport);
    scrollStyle(area.layer, "--scroll-left", style.paddingLeft);
    scrollStyle(area.layer, "--scroll-top", style.paddingTop);
    refreshScrollbars(area);
  }

  function scrollToPosition(area, axis, position) {
    if (axis === "horizontal") area.viewport.scrollLeft = position;
    else area.viewport.scrollTop = position;
    refreshScrollbars(area);
  }

  const scrollResize = new ResizeObserver((entries) => {
    const areas = new Set(entries.map((entry) => scrollAreas.get(entry.target)));
    for (const area of areas) measureScrollbars(area);
  });

  for (const [index, layer] of [...root.querySelectorAll("[data-scrollbars]")].entries()) {
    const viewport = layer.dataset.scrollTarget ? document.getElementById(layer.dataset.scrollTarget) : layer.parentElement;
    const rails = [...layer.querySelectorAll("[data-scroll-axis]")];
    const area = { viewport, layer, rails };
    if (!viewport.id) viewport.id = `artifact-scroll-${index}`;
    viewport.dataset.scrollEnhanced = "true";
    for (const rail of rails) {
      rail.setAttribute("aria-controls", viewport.id);
      scrollAreas.set(rail, area);
    }
    scrollAreas.set(viewport, area);
    scrollResize.observe(viewport);
    const content = viewport.querySelector(":scope > .scroll-area-content, :scope > [data-gtl-horizontal-size]");
    if (content) { scrollAreas.set(content, area); scrollResize.observe(content); }
    measureScrollbars(area);
  }

  root.addEventListener("scroll", (event) => {
    const area = scrollAreas.get(event.target);
    if (area) refreshScrollbars(area);
  }, true);

  root.addEventListener("pointerdown", (event) => {
    const rail = event.target.closest("[data-scroll-axis]");
    const area = scrollAreas.get(rail);
    if (!area || event.button !== 0 || scrollbarDrag) return;
    event.preventDefault();
    rail.focus();
    const axis = rail.dataset.scrollAxis;
    const pointer = axis === "horizontal" ? event.clientX : event.clientY;
    const [size, content, position] = scrollMetrics(area, axis);
    const geometry = thumbGeometry(size, content, position);
    if (!rail.firstElementChild.contains(event.target)) {
      const rect = rail.getBoundingClientRect();
      const start = axis === "horizontal" ? rect.x : rect.y;
      scrollToPosition(area, axis, position + (pointer - start < geometry.offset ? -size : size));
      return;
    }
    area.layer.setPointerCapture(event.pointerId);
    rail.dataset.dragging = "true";
    scrollbarDrag = { area, rail, axis, pointer, position, geometry, id: event.pointerId };
  });

  root.addEventListener("pointermove", (event) => {
    if (!scrollbarDrag || scrollbarDrag.id !== event.pointerId) return;
    event.preventDefault();
    const { area, axis, pointer, position, geometry } = scrollbarDrag;
    const current = axis === "horizontal" ? event.clientX : event.clientY;
    scrollToPosition(area, axis, position + (geometry.travel > 0 ? (current - pointer) / geometry.travel * geometry.maximum : 0));
  });

  function finishScrollbarDrag() {
    if (!scrollbarDrag) return;
    const { area, rail, id } = scrollbarDrag;
    scrollbarDrag = null;
    delete rail.dataset.dragging;
    if (area.layer.hasPointerCapture(id)) area.layer.releasePointerCapture(id);
  }

  root.addEventListener("pointerup", finishScrollbarDrag);
  root.addEventListener("pointercancel", finishScrollbarDrag);
  root.addEventListener("lostpointercapture", finishScrollbarDrag);

  function scrollbarKeydown(event) {
    const rail = event.target.closest("[data-scroll-axis]");
    const area = scrollAreas.get(rail);
    if (!area) return false;
    const axis = rail.dataset.scrollAxis;
    const [size, content, position] = scrollMetrics(area, axis);
    let next;
    switch (event.key) {
      case "ArrowLeft": if (axis === "horizontal") next = position - 40; break;
      case "ArrowRight": if (axis === "horizontal") next = position + 40; break;
      case "ArrowUp": if (axis === "vertical") next = position - 40; break;
      case "ArrowDown": if (axis === "vertical") next = position + 40; break;
      case "PageUp": next = position - size; break;
      case "PageDown": next = position + size; break;
      case "Home": next = 0; break;
      case "End": next = content - size; break;
    }
    if (next === undefined) return false;
    event.preventDefault();
    scrollToPosition(area, axis, next);
    return true;
  }

  function artifactPanel(element) {
    return element.closest("[data-gtl-view-panel]");
  }

  function panelWorkspace(panel) {
    return panel.querySelector("[data-gtl-workspace]");
  }

  function hoverPopoverTarget(node) {
    const element = node instanceof Element ? node : node?.parentElement;
    const target = element?.closest("[data-gtl-hover-popover-target]");
    return target !== undefined && target !== null && root.contains(target)
      ? target
      : null;
  }

  function hoverPopover(target) {
    const id = target.dataset.gtlHoverPopoverId;
    if (id === undefined) return null;
    const popover = document.getElementById(id);
    return popover instanceof HTMLElement && target.contains(popover)
      ? popover
      : null;
  }

  function scheduleHoverPopover(target) {
    const previousTimer = hoverPopoverTimers.get(target);
    const popover = hoverPopover(target);
    if (
      previousTimer !== undefined ||
      (popover !== null && popover.matches(":popover-open"))
    ) {
      return;
    }
    const delay = Number.parseInt(
      target.dataset.gtlHoverPopoverDelayMs ?? "0",
      10,
    );
    hoverPopoverTimers.set(
      target,
      setTimeout(() => {
        hoverPopoverTimers.delete(target);
        if (
          !target.matches(":hover") &&
          !target.contains(document.activeElement)
        ) {
          return;
        }
        const popover = hoverPopover(target);
        if (popover !== null && !popover.matches(":popover-open")) {
          popover.showPopover();
        }
      }, Number.isNaN(delay) ? 0 : delay),
    );
  }

  function hideHoverPopover(target) {
    const timer = hoverPopoverTimers.get(target);
    if (timer !== undefined) clearTimeout(timer);
    hoverPopoverTimers.delete(target);
    const popover = hoverPopover(target);
    if (popover !== null && popover.matches(":popover-open")) {
      popover.hidePopover();
    }
  }

  function hoverPopoverInteractionEnded(target) {
    if (
      target.matches(":hover") || target.contains(document.activeElement)
    ) {
      return;
    }
    hideHoverPopover(target);
  }

  function setTabSelected(tab, selected) {
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
  }

  function selectView(selectedTab) {
    const selectedView = selectedTab.dataset.gtlView;
    if (selectedView === undefined) return;
    const current = root.querySelector(
      "[data-gtl-action='select-view'][aria-selected='true']",
    );
    if (current === selectedTab) return;

    for (
      const panel of root.querySelectorAll("[data-gtl-view-panel]")
    ) {
      const selected = panel.dataset.gtlView === selectedView;
      if (!selected && !panel.hidden) resetPanel(panel);
      panel.hidden = !selected;
    }
    for (
      const tab of root.querySelectorAll(
        "[data-gtl-action='select-view']",
      )
    ) {
      setTabSelected(tab, tab === selectedTab);
    }
    selectedTab.focus();
  }

  function toggleSidebar(panel, sidebar) {
    const workspace = panelWorkspace(panel);
    if (workspace === null) return;
    const hiddenPanel = workspace.querySelector(
      `.diff-workspace-${sidebar}-panel`,
    );
    const restoreFocus = hiddenPanel?.contains(document.activeElement);
    const attribute = `data-${sidebar}-sidebar-visible`;
    const visible = workspace.getAttribute(attribute) !== "true";
    for (const target of root.querySelectorAll("[data-gtl-workspace]")) {
      target.setAttribute(attribute, String(visible));
      const sidebarPanel = target.querySelector(`[data-sidebar-panel='${sidebar}']`);
      if (sidebarPanel !== null) {
        sidebarPanel.setAttribute("aria-hidden", String(!visible));
        sidebarPanel.toggleAttribute("inert", !visible);
      }
    }
    for (
      const control of root.querySelectorAll(
        `[data-sidebar-toggle='${sidebar}']`,
      )
    ) {
      control.setAttribute("aria-pressed", String(visible));
    }
    if (!visible && restoreFocus) {
      panel.querySelector(`[data-sidebar-toggle='${sidebar}']`)?.focus();
    }
  }

  function setFilesFolded(panel, folded) {
    const workspace = panelWorkspace(panel);
    if (workspace === null) return;
    workspace.dataset.gtlFilesFolded = String(folded);
    for (
      const file of workspace.querySelectorAll(
        "details[data-gtl-file],details[data-gtl-diff-file]",
      )
    ) {
      file.open = !folded;
    }
    for (
      const control of panel.querySelectorAll(
        "[data-gtl-action='toggle-files']",
      )
    ) {
      const label = control.querySelector("[data-gtl-files-label]");
      const accessibleLabel = folded
        ? labels.gtlLabelExpandAll
        : labels.gtlLabelCollapseAll;
      if (label !== null) label.textContent = accessibleLabel;
      control.setAttribute("aria-label", accessibleLabel);
      control.setAttribute("title", accessibleLabel);
    }
    if (folded) {
      const diffDocument = panel.querySelector("[data-gtl-diff-document]");
      if (diffDocument instanceof HTMLElement) diffDocument.scrollTop = 0;
    }
  }

  function pathFilterOptions(filter) {
    return Array.from(filter.querySelectorAll("[role='option']:not([hidden])"));
  }

  function selectPathFilterOption(filter, selected) {
    for (const option of filter.querySelectorAll("[role='option']")) {
      option.setAttribute("aria-selected", String(option === selected));
    }
    const input = filter.querySelector("input");
    if (selected === undefined) input?.removeAttribute("aria-activedescendant");
    else {
      input?.setAttribute("aria-activedescendant", selected.id);
      selected.scrollIntoView({ block: "nearest" });
    }
  }

  function filterFiles(panel, value) {
    const filter = panel.querySelector("[data-gtl-path-filter]");
    if (filter === null) return;
    const input = filter.querySelector("input");
    if (input !== null && input.value !== value) input.value = value;
    const normalized = value.toLowerCase();
    for (const option of filter.querySelectorAll("[role='option']")) {
      option.hidden = !(option.dataset.gtlFilterKey ?? "").includes(normalized);
    }
    const options = pathFilterOptions(filter);
    selectPathFilterOption(filter, options[0]);
    const empty = filter.querySelector("[data-gtl-path-filter-empty]");
    if (empty !== null) empty.hidden = options.length > 0;
  }

  function setPathFilterOpen(panel, open, restoreFocus = true) {
    const filter = panel.querySelector("[data-gtl-path-filter]");
    if (filter === null) return;
    filter.hidden = !open;
    const input = filter.querySelector("input");
    input?.setAttribute("aria-expanded", String(open));
    for (
      const control of panel.querySelectorAll(
        "[data-gtl-action='open-path-filter']",
      )
    ) {
      control.setAttribute("aria-expanded", String(open));
    }
    if (open) {
      for (const dialog of panel.querySelectorAll("dialog[open]")) {
        closeDialog(dialog, false);
      }
      input?.focus();
    } else if (restoreFocus) {
      const workspace = panelWorkspace(panel);
      if (workspace !== null) {
        workspace.setAttribute("tabindex", "-1");
        workspace.focus();
      }
    }
  }

  function closeDialog(dialog, restoreFocus) {
    if (dialog.open) dialog.close();
    const triggerId = dialog.dataset.gtlDialogTrigger;
    const trigger = triggerId === undefined
      ? null
      : document.getElementById(triggerId);
    if (trigger !== null) {
      trigger.setAttribute("aria-expanded", "false");
      if (restoreFocus) trigger.focus();
    }
  }

  function openDialog(trigger) {
    const panel = artifactPanel(trigger);
    const dialogId = trigger.getAttribute("aria-controls");
    if (panel === null || dialogId === null) return;
    const dialog = document.getElementById(dialogId);
    if (!(dialog instanceof HTMLDialogElement) || !panel.contains(dialog)) {
      return;
    }
    dialog.dataset.gtlDialogTrigger = trigger.id;
    trigger.setAttribute("aria-expanded", "true");
    if (!dialog.open) dialog.showModal();
    dialog.querySelector("[data-dialog-initial-focus]")?.focus();
  }

  function resetPanel(panel) {
    setFilesFolded(panel, false);
    setCopyContext(panel, true);
    filterFiles(panel, "");
    setPathFilterOpen(panel, false, false);
    const workspace = panelWorkspace(panel);
    if (workspace !== null) {
      for (
        const file of workspace.querySelectorAll(
          "details[data-gtl-file],details[data-gtl-diff-file]",
        )
      ) {
        file.open = file.dataset.gtlInitialOpen !== "false";
      }
    }
    for (const dialog of panel.querySelectorAll("dialog[open]")) {
      closeDialog(dialog, false);
    }
    for (
      const target of panel.querySelectorAll(
        "[data-gtl-hover-popover-target]",
      )
    ) {
      hideHoverPopover(target);
    }
    const copyContextFeedback = panel.querySelector(
      "[data-gtl-copy-context-feedback]",
    );
    if (copyContextFeedback !== null) {
      const timer = copyContextFeedbackTimers.get(copyContextFeedback);
      if (timer !== undefined) clearTimeout(timer);
      copyContextFeedbackTimers.delete(copyContextFeedback);
      copyContextFeedback.hidden = true;
    }
  }

  function navigateFile(action) {
    const panel = artifactPanel(action);
    const targetId = action.dataset.fileTarget;
    if (panel === null || targetId === undefined) return;
    const target = document.getElementById(targetId);
    if (!(target instanceof HTMLDetailsElement) || !panel.contains(target)) {
      return;
    }
    if (!target.open) {
      target.open = true;
    }
    target.scrollIntoView({ block: "start" });
    const dialog = action.closest("dialog");
    if (dialog instanceof HTMLDialogElement) closeDialog(dialog, true);

    const previousTimer = flashTimers.get(target);
    if (previousTimer !== undefined) clearTimeout(previousTimer);
    target.classList.add("outline", "outline-acc", "outline-offset-[-1px]");
    flashTimers.set(
      target,
      setTimeout(() => {
        target.classList.remove(
          "outline",
          "outline-acc",
          "outline-offset-[-1px]",
        );
        flashTimers.delete(target);
      }, 1200),
    );
  }

  function copyContextHeader(file, firstLine, lastLine) {
    let range = "";
    if (firstLine !== undefined && lastLine !== undefined) {
      range = firstLine === lastLine
        ? ", lines: " + firstLine
        : ", lines: " + firstLine + ".." + lastLine;
    }
    const leader = file.dataset.gtlCommentLeader ?? "//";
    const path = file.dataset.gtlPath ?? file.dataset.path ?? "";
    return leader + " * " + path + range;
  }

  function diffFileForNode(node) {
    const element = node instanceof Element ? node : node?.parentElement;
    return element?.closest("details[data-gtl-diff-file]") ?? null;
  }

  function selectedCodePayload(selection) {
    if (selection.isCollapsed || selection.rangeCount === 0) return null;
    const file = diffFileForNode(selection.anchorNode);
    if (file === null || file !== diffFileForNode(selection.focusNode)) {
      return null;
    }
    const lines = [];
    let firstLine;
    let lastLine;
    for (const row of file.querySelectorAll("[data-gtl-copy-line]")) {
      if (!selection.containsNode(row, true)) continue;
      const text = row.querySelector("[data-gtl-copy-text]");
      if (text === null) continue;
      lines.push(text.textContent ?? "");
      const lineNumber = row.dataset.gtlNewLine;
      if (lineNumber !== undefined && /^[1-9][0-9]*$/.test(lineNumber)) {
        firstLine ??= lineNumber;
        lastLine = lineNumber;
      }
    }
    if (lines.length === 0) return null;

    const lineRange = firstLine === undefined || lastLine === undefined
      ? undefined
      : firstLine === lastLine
      ? firstLine
      : firstLine + ".." + lastLine;
    return {
      file,
      lineRange,
      text: copyContextHeader(file, firstLine, lastLine) + "\n" +
        lines.join("\n"),
    };
  }

  function showCopyContextFeedback(file, lineRange) {
    const panel = artifactPanel(file);
    const feedback = panel?.querySelector(
      "[data-gtl-copy-context-feedback]",
    );
    if (feedback === null || feedback === undefined) return;
    feedback.textContent = lineRange === undefined
      ? labels.gtlLabelCopiedContext
      : labels.gtlLabelCopiedContextLines.replace("{lines}", lineRange);
    feedback.hidden = false;
    const previousTimer = copyContextFeedbackTimers.get(feedback);
    if (previousTimer !== undefined) clearTimeout(previousTimer);
    copyContextFeedbackTimers.set(
      feedback,
      setTimeout(() => {
        feedback.hidden = true;
        copyContextFeedbackTimers.delete(feedback);
      }, 1600),
    );
  }

  function copyPayload(action) {
    const kind = action.dataset.gtlCopy;
    const file = action.closest(
      "details[data-gtl-file],details[data-gtl-diff-file]",
    );
    if (kind === "commit") return action.dataset.gtlCopyValue ?? "";
    if (file === null) return "";
    if (kind === "path") {
      return file.dataset.gtlPath ?? file.dataset.path ?? "";
    }
    if (kind === "absolute") return file.dataset.gtlAbsolutePath ?? "";
    return "";
  }

  async function writeClipboard(value) {
    try {
      if (navigator.clipboard?.writeText !== undefined) {
        await navigator.clipboard.writeText(value);
        return true;
      }
    } catch {
      // Continue to the local textarea fallback.
    }
    const textarea = document.createElement("textarea");
    textarea.value = value;
    textarea.setAttribute("readonly", "");
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    document.body.append(textarea);
    textarea.select();
    let copied = false;
    try {
      copied = document.execCommand("copy");
    } catch {
      copied = false;
    }
    textarea.remove();
    return copied;
  }

  function setCopyFeedback(action, state) {
    const feedback = action.querySelector("[data-gtl-copy-feedback]");
    if (feedback === null) return false;
    feedback.dataset.state = state;
    feedback.textContent = state === "success"
      ? labels.gtlLabelCopied
      : state === "failure"
      ? labels.gtlLabelCopyFailed
      : "";
    return true;
  }

  function showCopyFeedback(action, copied) {
    if (!setCopyFeedback(action, copied ? "success" : "failure")) return;
    const previousTimer = feedbackTimers.get(action);
    if (previousTimer !== undefined) clearTimeout(previousTimer);
    feedbackTimers.set(
      action,
      setTimeout(() => {
        setCopyFeedback(action, "idle");
        feedbackTimers.delete(action);
      }, 1200),
    );
  }

  async function copy(action, feedback) {
    const copied = await writeClipboard(copyPayload(action));
    if (feedback) showCopyFeedback(action, copied);
  }

  root.addEventListener("mouseover", (event) => {
    const target = hoverPopoverTarget(event.target);
    if (target === null || target.contains(event.relatedTarget)) return;
    scheduleHoverPopover(target);
  });

  root.addEventListener("mouseout", (event) => {
    const target = hoverPopoverTarget(event.target);
    if (target === null || target.contains(event.relatedTarget)) return;
    hoverPopoverInteractionEnded(target);
  });

  root.addEventListener("focusin", (event) => {
    const target = hoverPopoverTarget(event.target);
    if (target === null || target.contains(event.relatedTarget)) return;
    scheduleHoverPopover(target);
  });

  root.addEventListener("focusout", (event) => {
    const target = hoverPopoverTarget(event.target);
    if (target === null || target.contains(event.relatedTarget)) return;
    hoverPopoverInteractionEnded(target);
  });

  root.addEventListener("copy", (event) => {
    const selection = window.getSelection();
    if (selection === null || event.clipboardData === null) return;
    const copied = selectedCodePayload(selection);
    if (copied === null) return;
    event.clipboardData.setData("text/plain", copied.text);
    event.preventDefault();
    showCopyContextFeedback(copied.file, copied.lineRange);
  });

  root.addEventListener("click", (event) => {
    if (!(event.target instanceof Element)) return;
    const action = event.target.closest(
      "[data-gtl-action],[data-gtl-copy]",
    );
    if (action === null || !root.contains(action)) return;
    const name = action.dataset.gtlAction ??
      (action.dataset.gtlCopy === undefined ? "" : "copy");

    switch (name) {
      case "select-view":
        event.preventDefault();
        selectView(action);
        break;
      case "toggle-files-sidebar":
      case "toggle-commits-sidebar": {
        event.preventDefault();
        const panel = artifactPanel(action);
        if (panel !== null) {
          toggleSidebar(
            panel,
            name === "toggle-files-sidebar" ? "files" : "commits",
          );
        }
        break;
      }
      case "toggle-files": {
        event.preventDefault();
        const panel = artifactPanel(action);
        const workspace = panel === null ? null : panelWorkspace(panel);
        if (panel !== null && workspace !== null) {
          setFilesFolded(
            panel,
            workspace.dataset.gtlFilesFolded !== "true",
          );
        }
        break;
      }
      case "open-path-filter":
      case "select-path-filter-file": {
        event.preventDefault();
        const panel = artifactPanel(action);
        if (panel !== null) {
          setPathFilterOpen(panel, name === "open-path-filter");
          if (name === "select-path-filter-file") navigateFile(action);
        }
        break;
      }
      case "navigate-file":
        event.preventDefault();
        navigateFile(action);
        break;
      case "open-dialog":
        event.preventDefault();
        openDialog(action);
        break;
      case "close-dialog": {
        event.preventDefault();
        const dialog = action.closest("dialog");
        if (dialog instanceof HTMLDialogElement) closeDialog(dialog, true);
        break;
      }
      case "copy":
        event.stopPropagation();
        void copy(action, action.dataset.gtlCopy !== "commit");
        action.closest("[popover]")?.hidePopover();
        break;
      case "copy-commit":
        event.preventDefault();
        event.stopPropagation();
        action.dataset.gtlCopy = "commit";
        void copy(action, false);
        break;
    }
  });

  root.addEventListener("mousedown", (event) => {
    if (
      event.target instanceof Element &&
      event.target.closest("[data-gtl-path-filter] [role='option']")
    ) event.preventDefault();
  });

  root.addEventListener("focusout", (event) => {
    if (!(event.target instanceof Element)) return;
    const filter = event.target.closest("[data-gtl-path-filter]");
    const panel = artifactPanel(event.target);
    if (
      panel !== null && filter !== null && !filter.hidden &&
      !filter.contains(event.relatedTarget)
    ) setPathFilterOpen(panel, false, false);
  });

  document.addEventListener("keydown", (event) => {
    if (scrollbarKeydown(event)) return;
    if (!(event.target instanceof Element) || event.isComposing) return;
    const panel = artifactPanel(event.target) ??
      root.querySelector("[data-gtl-view-panel]:not([hidden])");
    if (panel === null) return;
    if (
      event.key.toLowerCase() === "b" && event.ctrlKey && !event.shiftKey &&
      !event.metaKey
    ) {
      event.preventDefault();
      if (event.repeat) return;
      const sidebar = event.altKey ? "commits" : "files";
      if (window.matchMedia("(min-width: 1025px)").matches) {
        toggleSidebar(panel, sidebar);
      } else {
        const trigger = panel.querySelector(
          `[data-gtl-action='open-dialog'][id$='-${sidebar}-trigger']`,
        );
        if (trigger !== null) {
          const dialog = document.getElementById(
            trigger.getAttribute("aria-controls"),
          );
          if (dialog instanceof HTMLDialogElement && dialog.open) {
            closeDialog(dialog, true);
          } else {
            for (
              const open of panel.querySelectorAll(
                "dialog[data-gtl-dialog][open]",
              )
            ) closeDialog(open, false);
            openDialog(trigger);
          }
        }
      }
      return;
    }
    const filter = event.target.closest("[data-gtl-path-filter]");
    if (
      filter !== null &&
      ["Escape", "ArrowDown", "ArrowUp", "Enter"].includes(event.key)
    ) {
      event.preventDefault();
      if (event.key === "Escape") setPathFilterOpen(panel, false);
      else {
        const options = pathFilterOptions(filter);
        const selected = options.findIndex((option) =>
          option.getAttribute("aria-selected") === "true"
        );
        if (event.key === "Enter" && options[selected] !== undefined) {
          setPathFilterOpen(panel, false);
          navigateFile(options[selected]);
        } else if (options.length > 0 && event.key !== "Enter") {
          const step = event.key === "ArrowDown" ? 1 : -1;
          selectPathFilterOption(
            filter,
            options[
              (Math.max(selected, 0) + step + options.length) % options.length
            ],
          );
        }
      }
    } else if (
      event.key.toLowerCase() === "p" && event.ctrlKey && !event.altKey &&
      !event.shiftKey && !event.metaKey
    ) {
      event.preventDefault();
      setPathFilterOpen(panel, true);
    }
  });

  root.addEventListener("input", (event) => {
    const input = event.target;
    if (
      !(input instanceof HTMLInputElement) ||
      input.dataset.gtlAction !== "filter-files"
    ) {
      return;
    }
    const panel = artifactPanel(input);
    if (panel !== null) filterFiles(panel, input.value);
  });

  root.addEventListener(
    "cancel",
    (event) => {
      const dialog = event.target;
      if (!(dialog instanceof HTMLDialogElement)) return;
      event.preventDefault();
      closeDialog(dialog, true);
    },
    true,
  );
})();
