import { LitElement, html, css } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { TemplateResult } from "lit";
import { closeTabState, type Tab } from "./tab-state";

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
  static override styles = css`
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
      display: inline-flex;
      align-items: center;
      gap: 8px;
      min-width: 0;
      padding: 7px 8px 7px 14px;
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
    .tab-label {
      max-width: 22ch;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .tab-close {
      display: grid;
      place-items: center;
      width: 18px;
      height: 18px;
      padding: 0;
      border: 0;
      border-radius: 4px;
      color: #858585;
      background: transparent;
      cursor: pointer;
      font: inherit;
      line-height: 1;
    }
    .tab-close:hover {
      color: #fff;
      background: #3a3d3f;
    }
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

  override connectedCallback(): void {
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

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._unlisten?.();
    this._unlisten = undefined;
  }

  private labelFor(url: string): string {
    return url.replace("diff://", "").slice(0, 12);
  }

  private openTab(url: string, label = this.labelFor(url)): void {
    const existing = this.tabs.findIndex((t) => t.url === url);
    if (existing >= 0) {
      const tab = this.tabs[existing];
      if (tab !== undefined && tab.label !== label) {
        this.tabs = this.tabs.map((t, i) => (i === existing ? { ...t, label } : t));
      }
      this.active = existing;
      this.showHistory = false;
      return;
    }
    this.tabs = [...this.tabs, { url, label }];
    this.active = this.tabs.length - 1;
    this.showHistory = false;
  }

  private closeTab(index: number): void {
    const next = closeTabState(
      { tabs: this.tabs, active: this.active, showHistory: this.showHistory },
      index
    );
    this.tabs = [...next.tabs];
    this.active = next.active;
    this.showHistory = next.showHistory;
  }

  private historyTabLabel(entry: HistoryEntry): string {
    const title = entry.title.trim();
    return title !== "" && title !== "diff" && title !== "merge-diff"
      ? title
      : this.labelFor(entry.url);
  }

  private async openHistory(): Promise<void> {
    const { core } = tauriGlobal();
    this.history = await core.invoke<HistoryEntry[]>("list_history");
    this.showHistory = true;
  }

  override render(): TemplateResult {
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
              title=${t.label}
            >
              <span class="tab-label">${t.label}</span>
              <button
                class="tab-close"
                type="button"
                aria-label=${`Close ${t.label}`}
                title="Close tab"
                @click=${(event: MouseEvent) => {
                  event.stopPropagation();
                  this.closeTab(i);
                }}
              >
                ×
              </button>
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
              <div class="row" @click=${() => this.openTab(r.url, this.historyTabLabel(r))}>
                ${r.title} · <small>${r.range_label} · ${r.head_committed_at}</small>
              </div>
            `
          )}
        `
      )}
    </div>`;
  }
}
