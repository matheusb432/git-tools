import type { SplitRow, UnifiedRow } from "@/shared/api";

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
