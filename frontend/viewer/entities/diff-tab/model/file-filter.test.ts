import { expect, test } from "bun:test";
import type { TabMeta } from "@/shared/api";
import { filterFiles } from "./file-filter";

const files: TabMeta["files"] = [{
  path: "src/main.rs",
  status: "modified",
  added: 2,
  removed: 1,
  commits: ["abc1234"],
  has_full: true,
}];

test("file filtering respects path text and focused commit set", () => {
  expect(filterFiles(files, "main", new Set()).map((f) => f.path)).toEqual(["src/main.rs"]);
  expect(filterFiles(files, "", new Set(["abc1234"]))).toHaveLength(1);
  expect(filterFiles(files, "", new Set(["missing"]))).toHaveLength(0);
});
