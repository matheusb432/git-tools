import { LitElement, html, css } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { TemplateResult } from "lit";

type HistoryEntry = {
  readonly repo_id: string;
  readonly repo_name: string;
  readonly title: string;
  readonly range_label: string;
  readonly head_committed_at: string;
  readonly generated_at: string;
  readonly content_hash: string;
  readonly url: string;
};
type Tab = { readonly url: string; readonly label: string };

// ---------------------------------------------------------------------------
// Tauri IPC boundary — validated once here; downstream code trusts the types.
// withGlobalTauri injects window.__TAURI__ at runtime; its shape is stable but
// not declared in @types, so we describe only what this module uses.
// ---------------------------------------------------------------------------

type TauriCore = { invoke: <T>(command: string) => Promise<T> };
type TauriEvent = {
  listen: <P>(event: string, handler: (e: { payload: P }) => void) => Promise<() => void>;
};
type TauriGlobal = { core: TauriCore; event: TauriEvent };

/** Validated accessor — throws only when the host is misconfigured (no __TAURI__). */
function tauriGlobal(): TauriGlobal {
  const w = window as unknown as Record<string, unknown>;
  const t = w["__TAURI__"];
  if (
    typeof t !== "object" ||
    t === null ||
    typeof (t as Record<string, unknown>)["core"] !== "object" ||
    typeof (t as Record<string, unknown>)["event"] !== "object"
  ) {
    throw new Error("__TAURI__ is not available — must run inside a Tauri webview");
  }
  // Single validated cast: we've confirmed the shape; inner generics are safe.
  return t as unknown as TauriGlobal;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

@customElement("gtl-shell")
export class GtlShell extends LitElement {
  static styles = css`
    :host {
      display: grid;
      grid-template-rows: auto 1fr;
      height: 100%;
      background: #1e1e1e;
      color: #d4d4d4;
      font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto,
        "Helvetica Neue", Arial, sans-serif;
      font-size: 14px;
      -webkit-font-smoothing: antialiased;
    }
    .mono {
      font-family: ui-monospace, "SF Mono", "JetBrains Mono", "Fira Code", Menlo,
        Consolas, monospace;
    }
    .tabs {
      display: flex;
      gap: 2px;
      background: #181818;
      padding: 6px 8px 0;
      overflow-x: auto;
      border-bottom: 1px solid #2d2d2d;
    }
    .tab {
      padding: 7px 14px;
      color: #9d9d9d;
      background: #232323;
      border-radius: 7px 7px 0 0;
      cursor: pointer;
      white-space: nowrap;
      font-family: ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace;
      font-size: 12.5px;
      transition: background 0.12s ease, color 0.12s ease;
    }
    .tab:hover { color: #d4d4d4; }
    .tab[active] { background: #1e1e1e; color: #fff; box-shadow: inset 0 2px 0 #569cd6; }
    .tab.history { margin-left: auto; font-family: inherit; font-weight: 600; }
    iframe { border: 0; width: 100%; height: 100%; background: #1e1e1e; }
    .panel { padding: 18px 22px; overflow: auto; }
    .panel code {
      font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
      background: #2a2a2a;
      padding: 1px 6px;
      border-radius: 4px;
    }
    .repo {
      margin: 20px 0 8px;
      font-size: 12px;
      font-weight: 700;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      color: #569cd6;
    }
    .repo:first-child { margin-top: 0; }
    .row {
      padding: 9px 12px;
      cursor: pointer;
      border-radius: 6px;
      border: 1px solid transparent;
      line-height: 1.5;
    }
    .row:hover { background: #2a2d2e; border-color: #333; }
    .row small {
      color: #858585;
      font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
      font-size: 11.5px;
    }
  `;

  @state() private tabs: Tab[] = [];
  @state() private active = 0;
  @state() private showHistory = false;
  @state() private history: HistoryEntry[] = [];

  private _unlisten: (() => void) | undefined;

  connectedCallback(): void {
    super.connectedCallback();
    const { core, event } = tauriGlobal();
    // Drain cold-start / early-arriving diffs, then live-listen for forwards.
    core
      .invoke<string[]>("drain_pending_diffs")
      .then((urls) => urls.forEach((u) => this.openTab(u)))
      .catch(console.error);
    event
      .listen<string>("open-diff", (e) => this.openTab(e.payload))
      .then((unlisten) => {
        this._unlisten = unlisten;
      })
      .catch(console.error);
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this._unlisten?.();
    this._unlisten = undefined;
  }

  private labelFor(url: string): string {
    return url.replace("diff://", "").slice(0, 12);
  }

  private openTab(url: string): void {
    const existing = this.tabs.findIndex((t) => t.url === url);
    if (existing >= 0) {
      this.active = existing;
      this.showHistory = false;
      return;
    }
    this.tabs = [...this.tabs, { url, label: this.labelFor(url) }];
    this.active = this.tabs.length - 1;
    this.showHistory = false;
  }

  private async openHistory(): Promise<void> {
    const { core } = tauriGlobal();
    this.history = await core.invoke<HistoryEntry[]>("list_history");
    this.showHistory = true;
  }

  render(): TemplateResult {
    return html`
      <div class="tabs">
        ${this.tabs.map(
          (t, i) => html`
            <div
              class="tab"
              ?active=${!this.showHistory && i === this.active}
              @click=${() => {
                this.active = i;
                this.showHistory = false;
              }}
            >
              ${t.label}
            </div>
          `
        )}
        <div
          class="tab history"
          ?active=${this.showHistory}
          @click=${() => this.openHistory()}
        >
          History
        </div>
      </div>
      ${this.showHistory ? this.renderHistory() : this.renderContent()}
    `;
  }

  private renderContent(): TemplateResult {
    const tab = this.tabs[this.active];
    return tab
      ? html`<iframe src=${tab.url}></iframe>`
      : html`<div class="panel">
          No diff open. Run <code>gtl diff</code> or pick from History.
        </div>`;
  }

  private renderHistory(): TemplateResult {
    const byRepo = new Map<string, HistoryEntry[]>();
    for (const e of this.history) {
      const k = e.repo_name || e.repo_id;
      const existing = byRepo.get(k);
      if (existing !== undefined) {
        existing.push(e);
      } else {
        byRepo.set(k, [e]);
      }
    }
    return html`<div class="panel">
      ${[...byRepo.entries()].map(
        ([repo, rows]) => html`
          <div class="repo">${repo}</div>
          ${rows.map(
            (r) => html`
              <div class="row" @click=${() => this.openTab(r.url)}>
                ${r.title} · <small>${r.range_label} · ${r.head_committed_at}</small>
              </div>
            `
          )}
        `
      )}
    </div>`;
  }
}
