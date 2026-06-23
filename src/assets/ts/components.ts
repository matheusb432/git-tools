import { LitElement, html, css } from "lit";
import { copyText } from "./clipboard";

const THEMES = ["dark", "light", "hearth"] as const;
type Theme = (typeof THEMES)[number];

class ThemeSwitch extends LitElement {
  static properties = { theme: { type: String } };

  static styles = css`
    :host{display:inline-flex;align-items:center;gap:6px}
    label{color:var(--ink-2);font-size:11px;letter-spacing:.04em}
    select{
      font:inherit;font-size:11px;color:var(--ink);background:var(--surface-2);
      border:1px solid var(--line-2);border-radius:4px;padding:2px 6px;cursor:pointer;
    }
    select:hover{border-color:var(--acc-line)}
  `;

  declare theme: Theme;

  constructor() {
    super();
    let saved: string | null = null;
    try { saved = localStorage.getItem("gtl-theme"); } catch (_e) {}
    this.theme = (THEMES as readonly string[]).includes(saved ?? "") ? (saved as Theme) : "dark";
    this.apply();
  }

  apply(): void {
    document.documentElement.dataset["theme"] = this.theme;
    try { localStorage.setItem("gtl-theme", this.theme); } catch (_e) {}
  }

  onChange(e: Event): void {
    this.theme = (e.target as HTMLSelectElement).value as Theme;
    this.apply();
  }

  render() {
    return html`
      <label>theme</label>
      <select @change=${this.onChange}>
        ${THEMES.map((t) => html`<option value=${t} ?selected=${t === this.theme}>${t}</option>`)}
      </select>
    `;
  }
}

customElements.define("theme-switch", ThemeSwitch);

export function extractCopyText(file: Element): string {
  const rows = file.querySelectorAll(".diff:not([hidden]) .dl-add, .diff:not([hidden]) .dl-ctx");
  const out: string[] = [];
  let firstLine: number | null = null;
  let lastLine: number | null = null;
  rows.forEach((row) => {
    const code = row.querySelector("code");
    if (!code) return;
    // ! prefer .code-text so a long line's "expand" affordance label never enters the copy
    const text = (code.querySelector(".code-text") ?? code).textContent ?? "";
    out.push(text.length && (text[0] === "+" || text[0] === " ") ? text.slice(1) : text);
    const lns = row.querySelectorAll(".ln");
    const n = lns.length > 1 ? parseInt(lns[1]?.textContent ?? "", 10) : NaN;
    if (!Number.isNaN(n)) { if (firstLine === null) firstLine = n; lastLine = n; }
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

type CopyState = "ok" | "err" | "";

class CopyButton extends LitElement {
  static properties = {
    value: { type: String },
    label: { type: String },
    mode: { type: String },
    state: { type: String },
  };

  static styles = css`
    :host{display:inline-flex}
    button{
      font:inherit;font-size:10px;letter-spacing:.04em;color:var(--acc);
      background:var(--acc-soft);border:1px solid var(--acc-line);border-radius:4px;
      padding:1px 6px;cursor:pointer;
    }
    button:hover{color:var(--bg);background:var(--acc);border-color:var(--acc)}
    button[data-state="ok"]{color:var(--bg);background:var(--add);border-color:var(--add)}
    button[data-state="err"]{color:var(--bg);background:var(--del);border-color:var(--del)}
  `;

  declare value: string;
  declare label: string;
  declare mode: string;
  declare state: CopyState;

  constructor() {
    super();
    this.value = "";
    this.label = "copy";
    this.mode = "";
    this.state = "";
  }

  payload(): string {
    if (this.mode !== "code") {
      return this.value;
    }
    return this.fileCode();
  }

  fileCode(): string {
    const file = this.closest("details.file");
    return file ? extractCopyText(file) : "";
  }

  onClick(): void {
    Promise.resolve(copyText(this.payload())).then((ok) => {
      this.state = ok ? "ok" : "err";
      setTimeout(() => { this.state = ""; }, 1200);
    });
  }

  render() {
    const text = this.state === "ok" ? "copied" : this.state === "err" ? "failed" : this.label;
    return html`<button type="button" data-state=${this.state} title=${this.label} @click=${this.onClick}>${text}</button>`;
  }
}

customElements.define("copy-button", CopyButton);
