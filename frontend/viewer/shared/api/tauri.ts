export type HistoryEntry = {
  readonly repo_id: string;
  readonly repo_name: string;
  readonly title: string;
  readonly range_label: string;
  readonly head_committed_at: string;
  readonly generated_at: string;
  readonly content_hash: string;
  readonly kind: string;
  readonly byte_size: number;
  readonly url: string;
};

type TauriCore = { readonly invoke: <T>(command: string) => Promise<T> };
type TauriEvent = {
  readonly listen: <P>(event: string, handler: (e: { readonly payload: P }) => void) => Promise<() => void>;
};
type TauriGlobal = { readonly core: TauriCore; readonly event: TauriEvent };

function record(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null ? (value as Record<string, unknown>) : undefined;
}

export function tauriGlobal(): TauriGlobal {
  const root = record(window);
  const tauri = record(root?.["__TAURI__"]);
  const core = record(tauri?.["core"]);
  const event = record(tauri?.["event"]);
  if (typeof core?.["invoke"] !== "function" || typeof event?.["listen"] !== "function") {
    throw new Error("__TAURI__ is not available; gtl-viewer must run inside a Tauri webview");
  }
  return {
    core: { invoke: <T>(command: string) => Promise.resolve((core["invoke"] as (c: string) => Promise<T>)(command)) },
    event: {
      listen: <P>(name: string, handler: (e: { readonly payload: P }) => void) =>
        Promise.resolve((event["listen"] as (n: string, h: (e: { readonly payload: P }) => void) => Promise<() => void>)(name, handler)),
    },
  };
}

export async function drainPendingDiffs(): Promise<string[]> {
  const urls = await tauriGlobal().core.invoke<unknown>("drain_pending_diffs");
  return Array.isArray(urls) ? urls.filter((u): u is string => typeof u === "string") : [];
}

export function listenOpenDiff(handler: (url: string) => void): Promise<() => void> {
  return tauriGlobal().event.listen<unknown>("open-diff", (e) => {
    if (typeof e.payload === "string") handler(e.payload);
  });
}

export async function listHistory(): Promise<HistoryEntry[]> {
  const rows = await tauriGlobal().core.invoke<unknown>("list_history");
  return Array.isArray(rows) ? rows.filter(isHistoryEntry) : [];
}

export function isHistoryEntry(value: unknown): value is HistoryEntry {
  const row = record(value);
  if (row === undefined) return false;
  return (
    typeof row["repo_id"] === "string" &&
    typeof row["repo_name"] === "string" &&
    typeof row["title"] === "string" &&
    typeof row["range_label"] === "string" &&
    typeof row["head_committed_at"] === "string" &&
    typeof row["generated_at"] === "string" &&
    typeof row["content_hash"] === "string" &&
    typeof row["kind"] === "string" &&
    typeof row["byte_size"] === "number" &&
    typeof row["url"] === "string"
  );
}
