import type { FileSummary } from "@/shared/api";

function matchesPath(file: FileSummary, text: string): boolean {
  return text === "" || file.path.toLowerCase().includes(text);
}

function matchesFocusedCommits(file: FileSummary, focusedCommits: ReadonlySet<string>): boolean {
  return focusedCommits.size === 0 || file.commits.some((sha) => focusedCommits.has(sha));
}

export function filterFiles(
  files: readonly FileSummary[],
  filterText: string,
  focusedCommits: ReadonlySet<string>,
): readonly FileSummary[] {
  const text = filterText.trim().toLowerCase();
  return files.filter((file) => matchesPath(file, text) && matchesFocusedCommits(file, focusedCommits));
}
