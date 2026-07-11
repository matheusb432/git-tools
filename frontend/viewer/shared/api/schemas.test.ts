import { expect, test } from "bun:test";
import { openRecipesWireSchema, openedTabSchema, rowsPageSchema, sourceProbeSchema } from "./index";

const minimalMeta = {
  tab_id: 7,
  batch_id: "batch-1",
  title: "diff",
  repo_name: "repo",
  repo_root: "/repo",
  branch: "main",
  upstream: "origin/main",
  cmd_lead: "git diff ",
  cmd_range: "origin/main..HEAD",
  cmd_trail: "",
  commits_label: "1 commit",
  foot_cmd: "git diff",
  foot_note: "",
  is_empty: false,
  commits: [],
  files: [],
};

test("source probe uses exact discriminants", () => {
  expect(sourceProbeSchema.parse({ outcome: "ok" })).toEqual({ outcome: "ok" });
  expect(sourceProbeSchema.parse({ outcome: "broken", code: "DirNotFound", reason: "gone" })).toEqual({
    outcome: "broken",
    code: "DirNotFound",
    reason: "gone",
  });
  expect(() => sourceProbeSchema.parse({ outcome: "unknown" })).toThrow();
  expect(() => sourceProbeSchema.parse({ outcome: "broken", code: "DirNotFound" })).toThrow();
});

test("opened tab enforces matching nested tab ids", () => {
  expect(openedTabSchema.parse({ tab_id: 7, meta: minimalMeta }).tab_id).toBe(7);
  expect(() => openedTabSchema.parse({ tab_id: 7, meta: { ...minimalMeta, tab_id: 8 } })).toThrow();
});

test("row pages reject unknown row variants", () => {
  expect(() =>
    rowsPageSchema.parse({
      total: 1,
      layout: "unified",
      rows: [{ kind: "unknown", old_no: null, new_no: 1, text: "x", owner: null, long_len: null }],
    }),
  ).toThrow();
});

test("recipe batches decode snake case once at the edge", () => {
  const decoded = openRecipesWireSchema.parse({
    batch_id: "batch-1",
    recipes: [{ source: { kind: "LocalRepo", value: "/repo" }, op: { op: "squash-preview" } }],
  });
  expect(decoded.batchId).toBe("batch-1");
});
