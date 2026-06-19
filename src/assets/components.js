// ! No-build Lit: static `properties` + constructor-init (NEVER class fields — they
// ! shadow Lit's reactive accessors and break reactivity); read globals off globalThis.Lit.
// ! IIFE-wrapped so nothing leaks to global scope and collides with the minified Lit bundle's
// ! top-level single-letter globals (a redeclaration aborts the offending script).
(function(){
const {LitElement, html, css} = globalThis.Lit;

const THEMES = ['dark', 'light', 'hearth'];

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

  constructor() {
    super();
    let saved = null;
    try { saved = localStorage.getItem('gtl-theme'); } catch (e) {}
    this.theme = THEMES.includes(saved) ? saved : 'dark';
    this.apply();
  }

  apply() {
    document.documentElement.dataset.theme = this.theme;
    try { localStorage.setItem('gtl-theme', this.theme); } catch (e) {}
  }

  onChange(e) {
    this.theme = e.target.value;
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

customElements.define('theme-switch', ThemeSwitch);

// ! Copy helper: clipboard API can be blocked on file:// (origin null) — fall back to a
// ! hidden textarea + execCommand('copy'). Returns true on success.
function copyText(text) {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    return navigator.clipboard.writeText(text).then(() => true, () => execCopy(text));
  }
  return Promise.resolve(execCopy(text));
}

function execCopy(text) {
  try {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(ta);
    return ok;
  } catch (e) {
    return false;
  }
}

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

  constructor() {
    super();
    this.value = '';
    this.label = 'copy';
    this.mode = '';
    this.state = '';
  }

  payload() {
    if (this.mode !== 'code') {
      return this.value;
    }
    return this.fileCode();
  }

  // ! Walk up to this button's own details.file, read its already-rendered diff rows, and
  // ! rebuild paste-ready source: keep added/context rows, drop their leading marker char.
  fileCode() {
    const file = this.closest('details.file');
    if (!file) {
      return '';
    }
    const rows = file.querySelectorAll('.diff:not([hidden]) .dl-add, .diff:not([hidden]) .dl-ctx');
    const out = [];
    rows.forEach((row) => {
      const code = row.querySelector('code');
      if (!code) {
        return;
      }
      const text = code.textContent;
      // strip the single leading marker: '+' for adds, ' ' for context
      out.push(text.length && (text[0] === '+' || text[0] === ' ') ? text.slice(1) : text);
    });
    return out.join('\n');
  }

  onClick() {
    Promise.resolve(copyText(this.payload())).then((ok) => {
      this.state = ok ? 'ok' : 'err';
      setTimeout(() => { this.state = ''; }, 1200);
    });
  }

  render() {
    const text = this.state === 'ok' ? 'copied' : this.state === 'err' ? 'failed' : this.label;
    return html`<button type="button" data-state=${this.state} title=${this.label} @click=${this.onClick}>${text}</button>`;
  }
}

customElements.define('copy-button', CopyButton);
})();
