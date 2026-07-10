import type { SplitCell, SplitRow, UnifiedRow } from "@/shared/api";
import type { RowPageRequest } from "@/entities/diff-tab";

export function panePageErrorEntries(
  pageErrors: ReadonlyMap<string, string>,
  paneKey: string,
): readonly (readonly [string, string])[] {
  const prefix = `${paneKey}:`;
  return Array.from(pageErrors.entries()).filter(([cacheKey]) => cacheKey.startsWith(prefix));
}

export function retryablePanePageRequests(
  pageErrors: ReadonlyMap<string, string>,
  paneKey: string,
): readonly RowPageRequest[] {
  return panePageErrorEntries(pageErrors, paneKey)
    .map(([cacheKey]) => {
      const parts = cacheKey.split(":");
      const pageStart = Number(parts.at(-2));
      const pageSize = Number(parts.at(-1));
      if (!Number.isFinite(pageStart) || !Number.isFinite(pageSize)) return null;
      return { pageStart, pageSize };
    })
    .filter((page): page is RowPageRequest => page !== null);
}

export function siblingLayout(layout: "unified" | "split"): "unified" | "split" {
  return layout === "unified" ? "split" : "unified";
}

export function unifiedLongRowKey(rowIndex: number): string {
  return `u:${rowIndex}`;
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
