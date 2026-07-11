import type { HistoryEntry } from "@/entities/diff";

/** Case-insensitive filter over repo name + title + range label. */
export function filterHistory(entries: readonly HistoryEntry[], query: string): HistoryEntry[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...entries];
  return entries.filter(
    (e) =>
      e.repo_name.toLowerCase().includes(q) ||
      e.title.toLowerCase().includes(q) ||
      e.range_label.toLowerCase().includes(q),
  );
}
