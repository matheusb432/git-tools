import { expect, test } from "bun:test";
import { filterHistory } from "./filter";
import type { HistoryEntry } from "@/entities/diff";

const entry = (over: Partial<HistoryEntry>): HistoryEntry => ({
  repo_id: "r",
  repo_name: "git-tools",
  title: "eod",
  range_label: "main...HEAD",
  head_committed_at: "t",
  generated_at: "t",
  content_hash: "h",
  kind: "3-dot",
  byte_size: 0,
  url: "diff://r/h",
  ...over,
});

test("empty query returns all", () => {
  const all = [entry({}), entry({ repo_name: "sample_project" })];
  expect(filterHistory(all, "")).toEqual(all);
});
test("matches on title, case-insensitive", () => {
  const all = [entry({ title: "eod" }), entry({ title: "fix prune" })];
  expect(filterHistory(all, "EOD").length).toBe(1);
});
test("matches on repo name", () => {
  const all = [entry({ repo_name: "git-tools" }), entry({ repo_name: "sample_project" })];
  const [first] = filterHistory(all, "sample_project");
  expect(first?.repo_name).toBe("sample_project");
});
test("matches on range label", () => {
  const all = [entry({ range_label: "main...HEAD" }), entry({ range_label: "main..HEAD" })];
  expect(filterHistory(all, "...").length).toBe(1);
});

test("trims surrounding query whitespace", () => {
  const all = [entry({ repo_name: "git-tools" }), entry({ repo_name: "sample_project" })];
  const [first] = filterHistory(all, "  sample_project  ");
  expect(first?.repo_name).toBe("sample_project");
});
