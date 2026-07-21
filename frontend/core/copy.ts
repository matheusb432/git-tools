export function extractCopyText(file: Element): string {
  const unified = file.querySelector(".diff-unified");
  const split = file.querySelector(".diff-split");
  const rows = unified ? Array.from(unified.querySelectorAll(".dl-add, .dl-ctx")) : [];
  const splitCodes = split ? Array.from(split.querySelectorAll(".sp-add, .dl > .sp-ctx:last-child")) : [];
  const out: string[] = [];
  let firstLine: number | null = null;
  let lastLine: number | null = null;

  rows.forEach((row) => {
    const code = row.querySelector("code");
    if (!code) return;
    const lns = row.querySelectorAll(".ln");
    const n = lns.length > 1 ? Number.parseInt(lns[1]?.textContent ?? "", 10) : Number.NaN;
    appendCode(code, n);
  });
  splitCodes.forEach((code) => {
    const lineNumber = code.previousElementSibling;
    const n = lineNumber?.matches(".ln") ? Number.parseInt(lineNumber.textContent ?? "", 10) : Number.NaN;
    appendCode(code, n);
  });

  function appendCode(code: Element, lineNumber: number): void {
    const text = (code.querySelector(".code-text") ?? code).textContent ?? "";
    out.push(text.length && (text[0] === "+" || text[0] === " ") ? text.slice(1) : text);
    if (Number.isNaN(lineNumber)) return;
    if (firstLine === null) firstLine = lineNumber;
    lastLine = lineNumber;
  }

  const code = out.join("\n");
  const layout = file.closest(".layout");
  const contextOn = !layout || layout.classList.contains("copy-ctx");
  if (!contextOn || !out.length) return code;

  const leader = file.getAttribute("data-comment") || "//";
  const path = file.getAttribute("data-path") || "";
  let header = `${leader} * ${path}`;
  if (firstLine !== null) {
    header += `, lines: ${firstLine === lastLine ? firstLine : `${firstLine}..${lastLine}`}`;
  }
  return `${header}\n${code}`;
}
