(() => {
  "use strict";

  const root = document.querySelector("[data-gtl-artifact-ready='true']");
  if (root === null) return;

  const feedbackTimers = new WeakMap();
  const copyContextFeedbackTimers = new WeakMap();
  const flashTimers = new WeakMap();
  const hoverPopoverTimers = new WeakMap();

  function classWords(value) {
    return (value ?? "").split(/\s+/).filter(Boolean);
  }

  function swapClasses(element, inactive, active) {
    const inactiveClasses = classWords(inactive);
    const activeClasses = classWords(active);
    element.classList.remove(...inactiveClasses, ...activeClasses);
    element.classList.add(...activeClasses);
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
    swapClasses(
      tab,
      selected
        ? tab.dataset.gtlUnselectedClasses
        : tab.dataset.gtlSelectedClasses,
      selected
        ? tab.dataset.gtlSelectedClasses
        : tab.dataset.gtlUnselectedClasses,
    );
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
      if (label !== null) {
        label.textContent = folded ? "Expand all" : "Collapse all";
      }
      const accessibleLabel = folded ? "Expand all" : "Collapse all";
      control.setAttribute("aria-label", accessibleLabel);
      control.setAttribute("title", accessibleLabel);
    }
    if (folded) {
      const diffDocument = panel.querySelector("[data-gtl-diff-document]");
      if (diffDocument instanceof HTMLElement) diffDocument.scrollTop = 0;
    }
  }

  function filterFiles(panel, value) {
    const normalized = value.toLowerCase();
    for (
      const input of panel.querySelectorAll(
        "input[data-gtl-action='filter-files']",
      )
    ) {
      if (input.value !== value) input.value = value;
    }
    for (
      const filePanel of panel.querySelectorAll("[data-gtl-file-panel]")
    ) {
      const leaves = Array.from(
        filePanel.querySelectorAll("[data-gtl-file-leaf]"),
      );
      for (const leaf of leaves) {
        leaf.hidden = !(leaf.dataset.gtlFilterKey ?? "").includes(normalized);
      }
      const directories = Array.from(
        filePanel.querySelectorAll("[data-gtl-file-directory]"),
      ).reverse();
      for (const directory of directories) {
        directory.hidden = !Array.from(
          directory.querySelectorAll("[data-gtl-file-leaf]"),
        ).some((leaf) => !leaf.hidden);
      }
      const empty = filePanel.querySelector("[data-gtl-files-empty]");
      if (empty !== null) {
        empty.hidden = !leaves.every((leaf) => leaf.hidden);
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

  function resetLongLine(container) {
    container.dataset.gtlExpanded = "false";
    const control = container.querySelector(
      "[data-gtl-action='toggle-long-line']",
    );
    const text = container.querySelector("[data-gtl-long-line-text]");
    if (control !== null) control.setAttribute("aria-expanded", "false");
    if (text !== null) {
      swapClasses(
        text,
        text.dataset.gtlExpandedClasses,
        text.dataset.gtlCollapsedClasses,
      );
    }
  }

  function resetPanel(panel) {
    setFilesFolded(panel, false);
    setCopyContext(panel, true);
    filterFiles(panel, "");
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
    for (
      const longLine of panel.querySelectorAll("[data-gtl-long-line]")
    ) {
      resetLongLine(longLine);
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
    target.open = true;
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
      ? "Copied with context"
      : "Copied with context - lines " + lineRange;
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
      ? "Copied"
      : state === "failure"
      ? "Failed"
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

  function toggleLongLine(action) {
    const container = action.closest("[data-gtl-long-line]");
    if (container === null) return;
    const expanded = container.dataset.gtlExpanded !== "true";
    container.dataset.gtlExpanded = String(expanded);
    action.setAttribute("aria-expanded", String(expanded));
    const text = container.querySelector("[data-gtl-long-line-text]");
    if (text !== null) {
      swapClasses(
        text,
        expanded
          ? text.dataset.gtlCollapsedClasses
          : text.dataset.gtlExpandedClasses,
        expanded
          ? text.dataset.gtlExpandedClasses
          : text.dataset.gtlCollapsedClasses,
      );
    }
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
      case "toggle-long-line":
        event.preventDefault();
        toggleLongLine(action);
        break;
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
