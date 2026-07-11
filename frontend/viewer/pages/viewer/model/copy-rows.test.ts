import { expect, test } from "bun:test";
import type { UnifiedRow } from "@/shared/api";
import { copyUnifiedRows } from "./copy-rows";

test("copy text includes visible row markers", () => {
  const rows: UnifiedRow[] = [{ kind: "add", old_no: null, new_no: 1, text: "new", owner: null, long_len: null }];
  expect(copyUnifiedRows(rows)).toBe("+new");
});
