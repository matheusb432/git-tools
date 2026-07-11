import type { SplitCell, SplitRow, UnifiedRow } from "@/shared/api";

export function siblingLayout(layout: "unified" | "split"): "unified" | "split" {
  return layout === "unified" ? "split" : "unified";
}

export function unifiedLongRowKey(rowIndex: number): string {
  return `u:${rowIndex}`;
}

export function scopedLongRowKey(paneKey: string, rowKey: string): string {
  return `${paneKey}|${rowKey}`;
}

export function expandedLongRowKeysForFile(
  expandedLongRows: ReadonlySet<string>,
  tabId: number,
  fileIdx: number,
): ReadonlySet<string> {
  const filePrefix = `${tabId}:${fileIdx}:`;
  return new Set([...expandedLongRows].filter((rowKey) => rowKey.startsWith(filePrefix)));
}

export function expandedLongRowsForPane(expandedLongRows: ReadonlySet<string>, paneKey: string): ReadonlySet<string> {
  const panePrefix = `${paneKey}|`;
  return new Set(
    [...expandedLongRows]
      .filter((rowKey) => rowKey.startsWith(panePrefix))
      .map((rowKey) => rowKey.slice(panePrefix.length)),
  );
}

export function sameStringSet(left: ReadonlySet<string>, right: ReadonlySet<string>): boolean {
  return left.size === right.size && [...left].every((value) => right.has(value));
}

export function splitBaseRowKey(rowIndex: number): string {
  return `s:${rowIndex}`;
}

export function splitLongRowKey(rowIndex: number, side: "old" | "new"): string {
  return `${splitBaseRowKey(rowIndex)}:${side}`;
}

export function splitMarker(side: "old" | "new", cell: SplitCell | null): "" | "-" | "+" {
  if (cell === null) return "";
  return side === "old" ? "-" : "+";
}

export function unifiedEachKey(rowIndex: number, row: UnifiedRow): string {
  return `${unifiedLongRowKey(rowIndex)}:${row.kind}:${row.old_no ?? "-"}:${row.new_no ?? "-"}`;
}

export function splitEachKey(rowIndex: number, row: SplitRow): string {
  return `${splitBaseRowKey(rowIndex)}:${row.kind}`;
}
