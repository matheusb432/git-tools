import type { HistoryEntry, TauriGlobal } from "../types";

function record(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null ? value as Record<string, unknown> : undefined;
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
    core: { invoke: <T>(command: string) => Promise.resolve((core["invoke"] as (command: string) => Promise<T>)(command)) },
    event: {
      listen: <P>(name: string, handler: (e: { readonly payload: P }) => void) =>
        Promise.resolve((event["listen"] as (name: string, handler: (e: { readonly payload: P }) => void) => Promise<() => void>)(name, handler)),
    },
  };
}

export async function drainPendingDiffs(): Promise<string[]> {
  const urls = await tauriGlobal().core.invoke<unknown>("drain_pending_diffs");
  return Array.isArray(urls) ? urls.filter((url): url is string => typeof url === "string") : [];
}

export function listenOpenDiff(handler: (url: string) => void): Promise<() => void> {
  return tauriGlobal().event.listen<unknown>("open-diff", (event) => {
    if (typeof event.payload === "string") handler(event.payload);
  });
}

export async function listHistory(): Promise<HistoryEntry[]> {
  const rows = await tauriGlobal().core.invoke<unknown>("list_history");
  if (!Array.isArray(rows)) return [];
  return rows.filter(isHistoryEntry);
}

function isHistoryEntry(value: unknown): value is HistoryEntry {
  const row = record(value);
  return (
    typeof row?.["repo_id"] === "string" &&
    typeof row["repo_name"] === "string" &&
    typeof row["title"] === "string" &&
    typeof row["range_label"] === "string" &&
    typeof row["head_committed_at"] === "string" &&
    typeof row["generated_at"] === "string" &&
    typeof row["content_hash"] === "string" &&
    typeof row["url"] === "string"
  );
}
