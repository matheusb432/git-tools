export function extractCopyText(file: Element): string {
  // ! Copy the whole file's changes from the unified pane regardless of the on-screen layout
  // ! (the split panes lay the same lines across two columns). Honour the full-file toggle.
  const full = document.documentElement.dataset["diffFull"] === "on";
  const unified =
    (full ? file.querySelector(".diff-unified.diff-full") : null) ??
    file.querySelector(".diff-unified.diff-compact");
  const rows = unified ? Array.from(unified.querySelectorAll(".dl-add, .dl-ctx")) : [];
  const out: string[] = [];
  let firstLine: number | null = null;
  let lastLine: number | null = null;

  rows.forEach((row) => {
    const code = row.querySelector("code");
    if (!code) return;
    const text = (code.querySelector(".code-text") ?? code).textContent ?? "";
    out.push(text.length && (text[0] === "+" || text[0] === " ") ? text.slice(1) : text);
    const lns = row.querySelectorAll(".ln");
    const n = lns.length > 1 ? Number.parseInt(lns[1]?.textContent ?? "", 10) : Number.NaN;
    if (!Number.isNaN(n)) {
      if (firstLine === null) firstLine = n;
      lastLine = n;
    }
  });

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
