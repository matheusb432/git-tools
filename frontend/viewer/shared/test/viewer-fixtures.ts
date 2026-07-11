import type { Recipe, RowsPage, TabMeta } from "@/shared/api";

export const recipe: Recipe = {
  source: { kind: "LocalRepo", value: "/repo" },
  op: { op: "diff", target: { target: "unpushed" } },
};

export const tabMeta: TabMeta = {
  tab_id: 7,
  batch_id: "batch-1",
  title: "repo diff",
  repo_name: "repo",
  repo_root: "/repo",
  branch: "main",
  upstream: "origin/main",
  cmd_lead: "git diff",
  cmd_range: "origin/main...HEAD",
  cmd_trail: "",
  commits_label: "1 commit",
  foot_cmd: "gtl diff",
  foot_note: "fixture",
  is_empty: false,
  commits: [
    {
      sha: "0123456789abcdef",
      subject: "test fixture",
      body: "",
      date: "2026-07-10",
      iso: "2026-07-10T12:00:00Z",
      parents: [],
      members: [],
      is_merge: false,
    },
  ],
  files: [
    {
      path: "src/main.rs",
      status: "M",
      added: 1,
      removed: 1,
      commits: ["0123456789abcdef"],
      has_full: true,
    },
  ],
};

export const twoFileTabMeta: TabMeta = {
  ...tabMeta,
  title: "two-file diff",
  files: [
    {
      path: "src/file-0.ts",
      status: "M",
      added: 1,
      removed: 1,
      commits: ["0123456789abcdef"],
      has_full: true,
    },
    {
      path: "src/file-1.ts",
      status: "M",
      added: 1,
      removed: 1,
      commits: ["0123456789abcdef"],
      has_full: true,
    },
  ],
};

export const unifiedPage: RowsPage = {
  total: 1,
  layout: "unified",
  rows: [{ kind: "add", old_no: null, new_no: 1, text: "let value = 1;", owner: null, long_len: null }],
};

export const splitPage: RowsPage = {
  total: 1,
  layout: "split",
  rows: [
    {
      kind: "pair",
      old: { no: 1, text: "let value = 0;", owner: null, spans: [], long_len: null },
      new: { no: 1, text: "let value = 1;", owner: null, spans: [], long_len: null },
    },
  ],
};
