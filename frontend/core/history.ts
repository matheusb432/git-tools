export type HistoryEntry = {
  readonly repo_id: string;
  readonly repo_name: string;
  readonly title: string;
  readonly range_label: string;
  readonly head_committed_at: string;
  readonly generated_at: string;
  readonly content_hash: string;
  readonly url: string;
};

export function groupHistoryByRepo(entries: readonly HistoryEntry[]): Map<string, HistoryEntry[]> {
  const byRepo = new Map<string, HistoryEntry[]>();
  for (const entry of entries) {
    const key = entry.repo_name || entry.repo_id;
    const rows = byRepo.get(key);
    if (rows !== undefined) {
      rows.push(entry);
    } else {
      byRepo.set(key, [entry]);
    }
  }
  return byRepo;
}

export function historyTabLabel(entry: HistoryEntry, fallback: (url: string) => string): string {
  const title = entry.title.trim();
  return title !== "" && title !== "diff" && title !== "merge-diff" ? title : fallback(entry.url);
}
