import { createStore } from "@xstate/store-svelte";

export type DiffReviewContext = {
  readonly filterText: string;
  readonly selectedFileIdx: number | null;
  readonly focusedCommits: ReadonlySet<string>;
  readonly collapsedFileIdxs: ReadonlySet<number>;
  readonly expandedLongRows: ReadonlySet<string>;
};

export type DiffReviewEventPayloads = {
  readonly "filter.changed": { readonly value: string };
  readonly "file.selected": { readonly fileIdx: number };
  readonly "commit.toggled": { readonly sha: string };
  readonly "commits.cleared": null;
  readonly "panel.toggled": { readonly fileIdx: number };
  readonly "panels.toggledAll": { readonly visibleFileIdxs: readonly number[] };
  readonly "longRow.toggled": { readonly rowKey: string };
};

function patchContext(context: DiffReviewContext, patch: Partial<DiffReviewContext>): DiffReviewContext {
  return { ...context, ...patch };
}

function toggledSet<T>(values: ReadonlySet<T>, value: T): ReadonlySet<T> {
  const next = new Set(values);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return next;
}

export function createDiffReviewStore() {
  return createStore<DiffReviewContext, DiffReviewEventPayloads>({
    context: {
      filterText: "",
      selectedFileIdx: null,
      focusedCommits: new Set<string>(),
      collapsedFileIdxs: new Set<number>(),
      expandedLongRows: new Set<string>(),
    },
    on: {
      "filter.changed": (context, event) => patchContext(context, { filterText: event.value }),
      "file.selected": (context, event) => {
        if (!context.collapsedFileIdxs.has(event.fileIdx)) {
          return patchContext(context, { selectedFileIdx: event.fileIdx });
        }

        const collapsedFileIdxs = new Set(context.collapsedFileIdxs);
        collapsedFileIdxs.delete(event.fileIdx);
        return patchContext(context, { selectedFileIdx: event.fileIdx, collapsedFileIdxs });
      },
      "commit.toggled": (context, event) =>
        patchContext(context, { focusedCommits: toggledSet(context.focusedCommits, event.sha) }),
      "commits.cleared": (context) => patchContext(context, { focusedCommits: new Set<string>() }),
      "panel.toggled": (context, event) =>
        patchContext(context, { collapsedFileIdxs: toggledSet(context.collapsedFileIdxs, event.fileIdx) }),
      "panels.toggledAll": (context, event) => {
        if (event.visibleFileIdxs.length === 0) return context;

        const collapsedFileIdxs = new Set(context.collapsedFileIdxs);
        const everyVisibleFileIsCollapsed = event.visibleFileIdxs.every((fileIdx) => collapsedFileIdxs.has(fileIdx));
        for (const fileIdx of event.visibleFileIdxs) {
          if (everyVisibleFileIsCollapsed) collapsedFileIdxs.delete(fileIdx);
          else collapsedFileIdxs.add(fileIdx);
        }
        return patchContext(context, { collapsedFileIdxs });
      },
      "longRow.toggled": (context, event) =>
        patchContext(context, { expandedLongRows: toggledSet(context.expandedLongRows, event.rowKey) }),
    },
  });
}

export type DiffReviewStore = ReturnType<typeof createDiffReviewStore>;
