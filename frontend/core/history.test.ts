import { expect, test } from "bun:test";
import { groupHistoryByRepo, historyTabLabel, type HistoryEntry } from "./history";

function entry(repo: string, title: string, url = "diff://repo/hash"): HistoryEntry {
  return {
    repo_id: repo,
    repo_name: repo,
    title,
    range_label: "main..topic",
    head_committed_at: "2026-06-24T00:00:00Z",
    generated_at: "2026-06-24T00:00:00Z",
    content_hash: "hash",
    url,
  };
}

test("groupHistoryByRepo groups rows under repo name", () => {
  expect([...groupHistoryByRepo([entry("repo-a", "diff"), entry("repo-a", "merge-diff")]).keys()]).toEqual(["repo-a"]);
});

test("historyTabLabel uses meaningful titles and falls back for generic commands", () => {
  expect(historyTabLabel(entry("repo-a", "review stack"), () => "fallback")).toBe("review stack");
  expect(historyTabLabel(entry("repo-a", "diff"), () => "fallback")).toBe("fallback");
});
