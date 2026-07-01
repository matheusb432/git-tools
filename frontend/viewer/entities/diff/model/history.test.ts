import { expect, test } from "bun:test";
import { groupHistoryByRepo, historyTabLabel, labelMap, timestampMap } from "./history";
import type { HistoryEntry } from "@/shared/api";

function entry(over: Partial<HistoryEntry>): HistoryEntry {
  return {
    repo_id: "r", repo_name: "git-tools", title: "diff", range_label: "main...HEAD",
    head_committed_at: "t", generated_at: "t", content_hash: "h",
    kind: "3-dot", byte_size: 0, url: "diff://r/h", ...over,
  };
}
const hash = (url: string) => url.split("/").pop()?.slice(0, 12) || "";

test("groups by repo_name, preserving order", () => {
  const g = groupHistoryByRepo([
    entry({ repo_name: "git-tools", url: "diff://r/a" }),
    entry({ repo_name: "sample_project", url: "diff://r/b" }),
    entry({ repo_name: "git-tools", url: "diff://r/c" }),
  ]);
  expect([...g.keys()]).toEqual(["git-tools", "sample_project"]);
  expect(g.get("git-tools")!.length).toBe(2);
});
test("falls back to repo_id when repo_name empty", () => {
  const g = groupHistoryByRepo([entry({ repo_name: "", repo_id: "abc" })]);
  expect([...g.keys()]).toEqual(["abc"]);
});
test("label uses the name, else the hash fallback", () => {
  expect(historyTabLabel(entry({ title: "eod" }), hash)).toBe("eod");
  expect(historyTabLabel(entry({ title: "diff", url: "diff://r/a5eb82f1c2d3aa" }), hash)).toBe("a5eb82f1c2d3");
  expect(historyTabLabel(entry({ title: "merge-diff", url: "diff://r/abcdef012345" }), hash)).toBe("abcdef012345");
});
test("labelMap maps url -> resolved label", () => {
  const m = labelMap([entry({ title: "eod", url: "diff://r/a" })], hash);
  expect(m.get("diff://r/a")).toBe("eod");
});
test("timestampMap maps url -> head_committed_at", () => {
  const m = timestampMap([entry({ url: "diff://r/a", head_committed_at: "2026-01-01T00:00:00Z" })]);
  expect(m.get("diff://r/a")).toBe("2026-01-01T00:00:00Z");
});
