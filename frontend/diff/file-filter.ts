export type FileVisibilityQuery = {
  /** Trimmed, lowercased name-filter text; empty matches every path. */
  readonly filterText: string;
  /** Selected commit shas, or null when no commit filter is active. */
  readonly activeShas: readonly string[] | null;
};

type FileFacts = {
  readonly pathLower: string;
  readonly commitShas: readonly string[];
};

// Attribute reads, sha splitting, and path lowering amortize across passes: the
// server-rendered attributes never change, and swaps replace the elements (and
// with them these keys).
const factsByFile = new WeakMap<Element, FileFacts>();

function factsOf(file: Element): FileFacts {
  let facts = factsByFile.get(file);
  if (!facts) {
    facts = {
      pathLower: (file.getAttribute("data-path") || "").toLowerCase(),
      commitShas: (file.getAttribute("data-commits") || "").split(" ").filter(Boolean),
    };
    factsByFile.set(file, facts);
  }
  return facts;
}

/**
 * One hidden flag per file, in file order: a file stays visible only when one of
 * its commits is selected (or no selection is active) and its path contains the
 * filter text.
 */
export function planFileVisibility(files: readonly Element[], query: FileVisibilityQuery): boolean[] {
  const selected = query.activeShas === null ? null : new Set(query.activeShas);
  const filterText = query.filterText;
  return files.map((file) => {
    const facts = factsOf(file);
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
