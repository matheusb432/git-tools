import { expect, test } from "bun:test";
import {
  type OpenRecipes,
  type Recipe,
  drainPendingRecipes,
  closeNativeTab,
  fileRows,
  getSetting,
  isOpenedTab,
  isOpenRecipes,
  isRecipe,
  isRowsPage,
  isSourceProbe,
  isTabMeta,
  listenOpenRecipe,
  openRecipe,
  probeSource,
  refreshTab,
  setSetting,
  tabMeta,
  tauriGlobal,
} from "./tauri";

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

test("setSetting accepts Tauri's null unit response", async () => {
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: { invoke: () => Promise.resolve(null) },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };

  await expect(setSetting("diff.layout", "split")).resolves.toBeUndefined();
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
          if (command === "set_setting") return Promise.resolve("yes");
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
  await expect(setSetting("diff.layout", "split")).rejects.toThrow("Malformed set_setting response");
});

test("isSourceProbe accepts ok and well-formed broken payloads, rejects the rest", () => {
  expect(isSourceProbe({ outcome: "ok" })).toBe(true);
  expect(isSourceProbe({ outcome: "broken", code: "DirNotFound", reason: "gone" })).toBe(true);
  expect(isSourceProbe({ outcome: "broken", code: "DirNotFound" })).toBe(false);
  expect(isSourceProbe({ outcome: "unknown" })).toBe(false);
  expect(isSourceProbe(null)).toBe(false);
});

test("probeSource invokes probe_source with the source kind and value", async () => {
  const calls: Array<{ command: string; args: unknown }> = [];
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: {
        invoke: (command: string, args: unknown) => {
          calls.push({ command, args });
          return Promise.resolve({ outcome: "broken", code: "DirNotFound", reason: "gone" });
        },
      },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };

  const result = await probeSource("LocalRepo", "/gone");

  expect(calls).toEqual([{ command: "probe_source", args: { sourceKind: "LocalRepo", sourceValue: "/gone" } }]);
  expect(result).toEqual({ outcome: "broken", code: "DirNotFound", reason: "gone" });
});

test("probeSource rejects a malformed payload", async () => {
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: { invoke: () => Promise.resolve({ outcome: "broken" }) },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };

  await expect(probeSource("LocalRepo", "/gone")).rejects.toThrow("Malformed probe_source response");
});

test("isRecipe accepts a well-formed recipe, rejects malformed ones", () => {
  expect(isRecipe(recipe)).toBe(true);
  expect(isRecipe({ source: recipe.source, op: { op: "unknown" } })).toBe(false);
  expect(isRecipe({ source: { kind: "GithubRepo", value: "o/r" }, op: recipe.op })).toBe(false);
  expect(isRecipe(null)).toBe(false);
});

test("isOpenRecipes accepts a batch keyed by the wire's snake_case batch_id", () => {
  expect(isOpenRecipes({ batch_id: "batch-1", recipes: [recipe] })).toBe(true);
  expect(isOpenRecipes({ batch_id: "batch-1", recipes: [] })).toBe(true);
});

test("isOpenRecipes rejects a missing/non-array recipes field or a malformed recipe", () => {
  expect(isOpenRecipes({ batch_id: "batch-1" })).toBe(false);
  expect(isOpenRecipes({ batch_id: "batch-1", recipes: "nope" })).toBe(false);
  expect(isOpenRecipes({ batch_id: "batch-1", recipes: [{ source: recipe.source, op: { op: "bogus" } }] })).toBe(
    false,
  );
  expect(isOpenRecipes({ recipes: [recipe] })).toBe(false);
  expect(isOpenRecipes(null)).toBe(false);
});

test("drainPendingRecipes maps the wire's batch_id onto batchId and drops malformed batches", async () => {
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: {
        invoke: () =>
          Promise.resolve([
            { batch_id: "batch-1", recipes: [recipe] },
            { batch_id: "batch-2", recipes: "not-an-array" },
          ]),
      },
      event: { listen: () => Promise.resolve(() => undefined) },
    },
  };

  expect(await drainPendingRecipes()).toEqual([{ batchId: "batch-1", recipes: [recipe] }]);
});

test("listenOpenRecipe subscribes to open-recipe and maps batch_id onto batchId", async () => {
  const calls: Array<{ event: string }> = [];
  const received: OpenRecipes[] = [];
  (globalThis.window as unknown as Record<string, unknown>) = {
    __TAURI__: {
      core: { invoke: () => Promise.resolve(undefined) },
      event: {
        listen: (event: string, handler: (e: { payload: unknown }) => void) => {
          calls.push({ event });
          handler({ payload: { batch_id: "batch-9", recipes: [recipe] } });
          return Promise.resolve(() => undefined);
        },
      },
    },
  };

  await listenOpenRecipe((batch) => received.push(batch));

  expect(calls).toEqual([{ event: "open-recipe" }]);
  expect(received).toEqual([{ batchId: "batch-9", recipes: [recipe] }]);
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
