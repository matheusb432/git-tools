import { expect, test } from "bun:test";
import {
  type Recipe,
  drainPendingDiffs,
  closeNativeTab,
  fileRows,
  getSetting,
  isHistoryEntry,
  isOpenedTab,
  isRowsPage,
  isTabMeta,
  listHistory,
  openRecipe,
  refreshTab,
  setSetting,
  tabMeta,
  tauriGlobal,
} from "./tauri";

const valid = {
  repo_id: "r",
  repo_name: "n",
  title: "t",
  range_label: "x",
  head_committed_at: "t",
  generated_at: "t",
  content_hash: "h",
  kind: "3-dot",
  byte_size: 4096,
  url: "diff://r/h",
};

const meta = {
  tab_id: 7,
  batch_id: "batch-1",
  title: "diff",
  repo_name: "repo",
  repo_root: "/repo",
  branch: "topic",
  upstream: "origin/main",
  cmd_lead: "git diff ",
  cmd_range: "origin/main..HEAD",
  cmd_trail: "",
  commits_label: "1 commit",
  foot_cmd: "git diff",
  foot_note: "",
  is_empty: false,
  commits: [
    {
      sha: "abc1234",
      subject: "feat: work",
      body: "body",
      date: "today",
      iso: "2026-07-08T00:00:00Z",
      parents: ["p1"],
      members: [],
      is_merge: false,
    },
  ],
  files: [
    {
      path: "src/main.rs",
      status: "modified",
      added: 2,
      removed: 1,
      commits: ["abc1234"],
      has_full: true,
    },
  ],
};

const recipe: Recipe = {
  source: { kind: "LocalRepo", value: "/repo" },
  op: { op: "diff", target: { target: "unpushed" } },
};

test("accepts a complete entry", () => {
  expect(isHistoryEntry(valid)).toBe(true);
});
test("rejects when kind missing", () => {
  const { kind: _kind, ...rest } = valid;
  expect(isHistoryEntry(rest)).toBe(false);
});
test("rejects when byte_size is not a number", () => {
  expect(isHistoryEntry({ ...valid, byte_size: "4096" })).toBe(false);
});
test("rejects non-objects", () => {
  expect(isHistoryEntry(null)).toBe(false);
  expect(isHistoryEntry("x")).toBe(false);
});

test("accepts native tab metadata and opened tab payloads", () => {
  expect(isTabMeta(meta)).toBe(true);
  expect(isOpenedTab({ tab_id: 7, meta })).toBe(true);
});

test("rejects malformed native metadata", () => {
  expect(isTabMeta({ ...meta, files: [{ path: "x" }] })).toBe(false);
  expect(isOpenedTab({ tab_id: "7", meta })).toBe(false);
  expect(isOpenedTab({ tab_id: 7, meta: { ...meta, tab_id: 8 } })).toBe(false);
});

test("accepts unified and split rows pages", () => {
  expect(
    isRowsPage({
      total: 1,
      layout: "unified",
      rows: [{ kind: "add", old_no: null, new_no: 1, text: "x", owner: null, long_len: null }],
    }),
  ).toBe(true);
  expect(
    isRowsPage({
      total: 1,
      layout: "split",
      rows: [{ kind: "pair", old: null, new: { no: 1, text: "x", owner: null, spans: [], long_len: null } }],
    }),
  ).toBe(true);
});

test("rejects unknown unified row kinds", () => {
  expect(
    isRowsPage({
      total: 1,
      layout: "unified",
      rows: [{ kind: "unknown", old_no: null, new_no: 1, text: "x", owner: null, long_len: null }],
    }),
  ).toBe(false);
});

test("tauriGlobal throws when __TAURI__ is absent", () => {
  (globalThis.window as unknown as Record<string, unknown>) = {};
  expect(() => tauriGlobal()).toThrow("__TAURI__ is not available");
});

test("drainPendingDiffs drops non-string payload items", async () => {
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: { invoke: () => Promise.resolve(["diff://repo/a", 7, null]) },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };
  expect(await drainPendingDiffs()).toEqual(["diff://repo/a"]);
});

test("listHistory drops malformed rows", async () => {
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: {
        invoke: () =>
          Promise.resolve([
            {
              repo_id: "repo",
              repo_name: "repo",
              title: "review",
              range_label: "main..topic",
              head_committed_at: "2026-06-24T00:00:00Z",
              generated_at: "2026-06-24T00:00:00Z",
              content_hash: "hash",
              kind: "2-dot",
              byte_size: 1234,
              url: "diff://repo/hash",
            },
            { repo_id: "broken" },
          ]),
      },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };
  expect(await listHistory()).toHaveLength(1);
});

test("native wrappers invoke exact Tauri command names and args", async () => {
  const calls: Array<{ command: string; args: unknown }> = [];
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: {
        invoke: (command: string, args: unknown) => {
          calls.push({ command, args });
          if (command === "open_recipe") return Promise.resolve({ tab_id: 7, meta });
          if (command === "tab_meta") return Promise.resolve(meta);
          if (command === "file_rows") return Promise.resolve({ total: 0, layout: "unified", rows: [] });
          if (command === "refresh_tab") return Promise.resolve(meta);
          if (command === "get_setting") return Promise.resolve("split");
          return Promise.resolve(undefined);
        },
      },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };

  await openRecipe(recipe, "batch-1");
  await tabMeta(7);
  await fileRows({ tabId: 7, fileIdx: 0, layout: "unified", full: false, start: 0, count: 80 });
  await refreshTab(7);
  await getSetting("diff.layout");
  await setSetting("diff.layout", "split");

  expect(calls.map((c) => c.command)).toEqual([
    "open_recipe",
    "tab_meta",
    "file_rows",
    "refresh_tab",
    "get_setting",
    "set_setting",
  ]);
  expect(calls[2]?.args).toEqual({ tabId: 7, fileIdx: 0, layout: "unified", full: false, start: 0, count: 80 });
});

test("native wrappers reject malformed payloads", async () => {
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: {
        invoke: (command: string) => {
          if (command === "open_recipe")
            return Promise.resolve({ tab_id: 7, meta: { ...meta, files: [{ path: "x" }] } });
          if (command === "tab_meta") return Promise.resolve({ ...meta, files: [{ path: "x" }] });
          if (command === "file_rows") {
            return Promise.resolve({
              total: 1,
              layout: "unified",
              rows: [{ kind: "unknown", old_no: null, new_no: 1, text: "x", owner: null, long_len: null }],
            });
          }
          if (command === "refresh_tab") return Promise.resolve({ ...meta, tab_id: "7" });
          if (command === "close_tab") return Promise.resolve("yes");
          return Promise.resolve(undefined);
        },
      },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };

  await expect(openRecipe(recipe, "batch-1")).rejects.toThrow("Malformed open_recipe response");
  await expect(tabMeta(7)).rejects.toThrow("Malformed tab_meta response");
  await expect(fileRows({ tabId: 7, fileIdx: 0, layout: "unified", full: false, start: 0, count: 80 })).rejects.toThrow(
    "Malformed file_rows response",
  );
  await expect(refreshTab(7)).rejects.toThrow("Malformed refresh_tab response");
  await expect(closeNativeTab(7)).rejects.toThrow("Malformed close_tab response");
});

test("closeNativeTab invokes close_tab with the tab id", async () => {
  const calls: Array<{ command: string; args: unknown }> = [];
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: {
        invoke: (command: string, args: unknown) => {
          calls.push({ command, args });
          return Promise.resolve(true);
        },
      },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };

  await expect(closeNativeTab(7)).resolves.toBe(true);
  expect(calls).toEqual([{ command: "close_tab", args: { tabId: 7 } }]);
});
