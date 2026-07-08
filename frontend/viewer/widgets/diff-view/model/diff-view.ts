import { rowPageKey, type RowPageCache, type RowPageRequest } from "@/entities/diff-tab";
import type { RowsPage, SplitRow, UnifiedRow } from "@/shared/api";

export type DiffLayout = "unified" | "split";

export type DiffViewState = {
  readonly layout: DiffLayout;
  readonly full: boolean;
  readonly filterText: string;
  readonly focusedCommits: ReadonlySet<string>;
};

export type ActivePaneKeyArgs = {
  readonly tabId: number;
  readonly fileIdx: number;
  readonly layout: DiffLayout;
  readonly full: boolean;
};

export type NextFileIndexArgs = {
  readonly current: number;
  readonly direction: "previous" | "next";
  readonly total: number;
};

export type CopyPageArgs = {
  readonly rowPageCache: RowPageCache;
  readonly tabId: number;
  readonly fileIdx: number;
  readonly layout: DiffLayout;
  readonly full: boolean;
};

export type MissingCopyPageRequestArgs = CopyPageArgs & {
  readonly pageSize: number;
  readonly total: number;
};

export type OrderedCopyPage = {
  readonly pageStart: number;
  readonly pageSize: number;
  readonly page: RowsPage;
};

export function layoutFromSetting(value: string | null): DiffLayout {
  return value === "split" ? "split" : "unified";
}

export function fullFromSetting(value: string | null): boolean {
  return value === "full";
}

export function settingFromLayout(layout: DiffLayout): string {
  return layout;
}

export function settingFromFull(full: boolean): string {
  return full ? "full" : "compact";
}

export function nextFileIndex({ current, direction, total }: NextFileIndexArgs): number {
  if (total <= 0) return 0;
  const normalizedCurrent = Math.max(0, Math.min(total - 1, Math.trunc(current)));
  if (direction === "previous") {
    return Math.max(0, normalizedCurrent - 1);
  }
  return Math.min(total - 1, normalizedCurrent + 1);
}

export function createDiffViewState(): DiffViewState {
  return {
    layout: "unified",
    full: false,
    filterText: "",
    focusedCommits: new Set<string>(),
  };
}

export function toggleLayout(state: DiffViewState): DiffViewState {
  return setLayout(state, state.layout === "unified" ? "split" : "unified");
}

export function setLayout(state: DiffViewState, layout: DiffLayout): DiffViewState {
  return { ...state, layout };
}

export function toggleFull(state: DiffViewState): DiffViewState {
  return { ...state, full: !state.full };
}

export function setFilterText(state: DiffViewState, filterText: string): DiffViewState {
  return { ...state, filterText };
}

export function focusCommit(state: DiffViewState, sha: string): DiffViewState {
  const focusedCommits = new Set(state.focusedCommits);
  focusedCommits.add(sha);
  return { ...state, focusedCommits };
}

export function toggleCommitFocus(state: DiffViewState, sha: string): DiffViewState {
  const focusedCommits = new Set(state.focusedCommits);
  if (focusedCommits.has(sha)) focusedCommits.delete(sha);
  else focusedCommits.add(sha);
  return { ...state, focusedCommits };
}

export function clearCommitFocus(state: DiffViewState): DiffViewState {
  return { ...state, focusedCommits: new Set<string>() };
}

export function activePaneKey({ tabId, fileIdx, layout, full }: ActivePaneKeyArgs): string {
  return `${tabId}:${fileIdx}:${layout}:${full ? "full" : "compact"}`;
}

export function orderedCopyPages({ rowPageCache, tabId, fileIdx, layout, full }: CopyPageArgs): readonly OrderedCopyPage[] {
  const prefix = `${activePaneKey({ tabId, fileIdx, layout, full })}:`;

  return [...rowPageCache.entries()]
    .filter(([key, page]) => key.startsWith(prefix) && page.layout === layout)
    .map(([key, page]) => {
      const parts = key.split(":");
      return {
        pageStart: Number(parts.at(-2)),
        pageSize: Number(parts.at(-1)),
        page,
      };
    })
    .filter((page) => Number.isFinite(page.pageStart) && Number.isFinite(page.pageSize))
    .sort((left, right) => left.pageStart - right.pageStart);
}

export function missingCopyPageRequests({
  rowPageCache,
  tabId,
  fileIdx,
  layout,
  full,
  pageSize,
  total,
}: MissingCopyPageRequestArgs): readonly RowPageRequest[] {
  const normalizedPageSize = Math.max(1, Math.trunc(pageSize));
  const requests: RowPageRequest[] = [];

  for (let pageStart = 0; pageStart < total; pageStart += normalizedPageSize) {
    const key = rowPageKey({ tabId, fileIdx, layout, full, pageStart, pageSize: normalizedPageSize });
    if (!rowPageCache.has(key)) {
      requests.push({ pageStart, pageSize: normalizedPageSize });
    }
  }

  return requests;
}

function unifiedMarker(row: UnifiedRow): string {
  switch (row.kind) {
    case "add":
      return "+";
    case "del":
      return "-";
    case "meta":
    case "hunk":
      return "@";
    case "context":
      return " ";
  }
}

export function copyUnifiedRows(rows: readonly UnifiedRow[]): string {
  return rows.map((row) => `${unifiedMarker(row)}${row.text}`).join("\n");
}

function splitCellText(prefix: string, text: string): string {
  return `${prefix}${text}`;
}

export function copySplitRows(rows: readonly SplitRow[]): string {
  return rows
    .map((row) => {
      switch (row.kind) {
        case "meta":
        case "hunk":
          return `@${row.text}`;
        case "context":
          return ` ${row.text}`;
        case "pair": {
          const oldText = row.old === null ? "-" : splitCellText("-", row.old.text);
          const newText = row.new === null ? "+" : splitCellText("+", row.new.text);
          return `${oldText}\t${newText}`;
        }
      }
    })
    .join("\n");
}
