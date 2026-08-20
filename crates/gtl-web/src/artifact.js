(() => {
  "use strict";

  const root = document.querySelector("[data-gtl-artifact-ready='true']");
  if (root === null) return;

  const feedbackTimers = new WeakMap();
  const flashTimers = new WeakMap();

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
      control.textContent = folded ? "Expand all" : "Collapse all";
    }
  }

  function setCopyContext(panel, enabled) {
    const workspace = panelWorkspace(panel);
    if (workspace === null) return;
    workspace.dataset.gtlCopyContext = String(enabled);
    for (
      const control of panel.querySelectorAll(
        "[data-gtl-action='toggle-copy-context']",
      )
    ) {
      control.setAttribute("aria-pressed", String(enabled));
      swapClasses(
        control,
        enabled
          ? control.dataset.gtlUnselectedClasses
          : control.dataset.gtlSelectedClasses,
        enabled
          ? control.dataset.gtlSelectedClasses
          : control.dataset.gtlUnselectedClasses,
      );
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

  function codePayload(file, includeContext) {
    const lines = [];
    let firstLine;
    let lastLine;
    for (const row of file.querySelectorAll("[data-gtl-copy-line]")) {
      const text = row.querySelector("[data-gtl-copy-text]");
      if (text === null) continue;
      lines.push(text.textContent ?? "");
      const lineNumber = row.dataset.gtlNewLine;
      if (lineNumber !== undefined && /^[1-9][0-9]*$/.test(lineNumber)) {
        firstLine ??= lineNumber;
        lastLine = lineNumber;
      }
    }
    const code = lines.join("\n");
    if (!includeContext || lines.length === 0) return code;

    let range = "";
    if (firstLine !== undefined && lastLine !== undefined) {
      range = firstLine === lastLine
        ? ", lines: " + firstLine
        : ", lines: " + firstLine + ".." + lastLine;
    }
    const leader = file.dataset.gtlCommentLeader ?? "//";
    const path = file.dataset.gtlPath ?? file.dataset.path ?? "";
    return leader + " * " + path + range + "\n" + code;
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
    if (kind === "code") {
      const panel = artifactPanel(action);
      const workspace = panel === null ? null : panelWorkspace(panel);
      return codePayload(file, workspace?.dataset.gtlCopyContext !== "false");
    }
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

  function showCopyFeedback(action, copied) {
    const idleLabel = action.dataset.gtlIdleLabel ??
      action.textContent ??
      "";
    action.dataset.gtlIdleLabel = idleLabel;
    action.textContent = copied ? "copied" : "failed";
    swapClasses(
      action,
      action.dataset.gtlIdleClasses,
      copied
        ? action.dataset.gtlSuccessClasses
        : action.dataset.gtlFailureClasses,
    );
    const previousTimer = feedbackTimers.get(action);
    if (previousTimer !== undefined) clearTimeout(previousTimer);
    feedbackTimers.set(
      action,
      setTimeout(() => {
        action.textContent = idleLabel;
        swapClasses(
          action,
          (action.dataset.gtlSuccessClasses ?? "") + " " +
            (action.dataset.gtlFailureClasses ?? ""),
          action.dataset.gtlIdleClasses,
        );
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
      case "toggle-copy-context": {
        event.preventDefault();
        const panel = artifactPanel(action);
        const workspace = panel === null ? null : panelWorkspace(panel);
        if (panel !== null && workspace !== null) {
          setCopyContext(
            panel,
            workspace.dataset.gtlCopyContext !== "true",
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
        event.preventDefault();
        event.stopPropagation();
        void copy(action, action.dataset.gtlCopy !== "commit");
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
