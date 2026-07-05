import type { HistoryEntry } from "@/shared/api";

export type { HistoryEntry };

export function historyTabLabel(entry: HistoryEntry, fallback: (url: string) => string): string {
  const title = entry.title.trim();
  return title !== "" && title !== "diff" && title !== "merge-diff" ? title : fallback(entry.url);
}

/** url -> tab label, for resolving live-opened tabs to their `-n` name. */
export function labelMap(entries: readonly HistoryEntry[], fallback: (url: string) => string): Map<string, string> {
  const map = new Map<string, string>();
  for (const e of entries) map.set(e.url, historyTabLabel(e, fallback));
  return map;
}

/** url -> `head_committed_at`, for ordering live-opened tabs by underlying diff recency. */
export function timestampMap(entries: readonly HistoryEntry[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const e of entries) map.set(e.url, e.head_committed_at);
  return map;
}
