/** The server-rendered attributes of one file, read once per mount. */
export type FileFacts = {
  readonly pathLower: string;
  readonly commitShas: readonly string[];
};

export type FileVisibilityQuery = {
  /** Trimmed, lowercased name-filter text; empty matches every path. */
  readonly filterText: string;
  /** Selected commit shas, or null when no commit filter is active. */
  readonly activeShas: ReadonlySet<string> | null;
};

/**
 * One hidden flag per file, in file order: a file stays visible only when one of
 * its commits is selected (or no selection is active) and its path contains the
 * filter text.
 */
export function planFileVisibility(files: readonly FileFacts[], query: FileVisibilityQuery): boolean[] {
  const selected = query.activeShas;
  const filterText = query.filterText;
  return files.map((facts) => {
    if (selected !== null && !selectionOwnsAny(selected, facts.commitShas)) return true;
    return filterText !== "" && !facts.pathLower.includes(filterText);
  });
}

function selectionOwnsAny(selected: ReadonlySet<string>, shas: readonly string[]): boolean {
  for (const sha of shas) {
    if (selected.has(sha)) return true;
  }
  return false;
}
