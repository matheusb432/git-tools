/** Clean source rebuilt from rendered diff rows, plus the new-side span those rows cover. */
export type CopiedRows = {
  /** One entry per copied source row. */
  readonly lines: readonly string[];
  /** `null` when no copied row carried a new-side line number. */
  readonly lineRange: string | null;
};

const NO_ELEMENTS: readonly Element[] = [];

/** The rendered line number, or null when the cell is absent or carries no digits. */
function parseLineNumber(text: string | null | undefined): number | null {
  const value = Number.parseInt(text ?? "", 10);
  return Number.isNaN(value) ? null : value;
}

/**
 * Reads rendered rows back as source, excluding long-line expander chrome. Unified rows are
 * marker-free; split rows retain their marker and remove it here. A non-null `keep` narrows
 * unified rows down to a selection and runs once per row.
 */
export function readCopiedRows(
  unifiedRows: Iterable<Element>,
  keep: ((row: Element) => boolean) | null,
  splitCodes: Iterable<Element> = NO_ELEMENTS,
): CopiedRows {
  const lines: string[] = [];
  let firstLine: number | null = null;
  let lastLine = 0;

  function codeText(code: Element): string {
    return (code.querySelector(".code-text") ?? code).textContent ?? "";
  }

  function append(text: string, lineNumber: number | null): void {
    lines.push(text);
    if (lineNumber === null) return;
    if (firstLine === null) firstLine = lineNumber;
    lastLine = lineNumber;
  }

  for (const row of unifiedRows) {
    if (keep && !keep(row)) continue;
    const code = row.querySelector("code");
    if (!code) continue;
    const lns = row.querySelectorAll(".ln");
    append(codeText(code), lns.length > 1 ? parseLineNumber(lns[1]?.textContent) : null);
  }
  for (const code of splitCodes) {
    const lineNumber = code.previousElementSibling;
    append(codeText(code).slice(1), lineNumber?.matches(".ln") ? parseLineNumber(lineNumber.textContent) : null);
  }

  return {
    lines,
    lineRange: firstLine === null ? null : firstLine === lastLine ? `${firstLine}` : `${firstLine}..${lastLine}`,
  };
}

/** A file outside any layout copies with context; inside one, the view's toggle decides. */
export function copyContextEnabled(file: Element): boolean {
  const layout = file.closest(".layout");
  return !layout || layout.classList.contains("copy-ctx");
}

/** The commented `path, lines` line prepended to a copy. */
export function copyHeader(file: Element, lineRange: string | null): string {
  const leader = file.getAttribute("data-comment") || "//";
  const path = file.getAttribute("data-path") || "";
  return `${leader} * ${path}${lineRange === null ? "" : `, lines: ${lineRange}`}`;
}

export function extractCopyText(file: Element): string {
  const unified = file.querySelector(".diff-unified");
  const split = file.querySelector(".diff-split");
  const copied = readCopiedRows(
    unified ? unified.querySelectorAll(".dl-add, .dl-ctx") : NO_ELEMENTS,
    null,
    split ? split.querySelectorAll(".sp-add, .dl > .sp-ctx:last-child") : NO_ELEMENTS,
  );

  const code = copied.lines.join("\n");
  if (!copied.lines.length || !copyContextEnabled(file)) return code;
  return `${copyHeader(file, copied.lineRange)}\n${code}`;
}
