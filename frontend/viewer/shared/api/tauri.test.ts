import { expect, test } from "bun:test";
import { isHistoryEntry, drainPendingDiffs, listHistory, tauriGlobal } from "./tauri";

const valid = {
  repo_id: "r", repo_name: "n", title: "t", range_label: "x",
  head_committed_at: "t", generated_at: "t", content_hash: "h",
  kind: "3-dot", byte_size: 4096, url: "diff://r/h",
};

test("accepts a complete entry", () => {
  expect(isHistoryEntry(valid)).toBe(true);
});
test("rejects when kind missing", () => {
  const { kind, ...rest } = valid;
  expect(isHistoryEntry(rest)).toBe(false);
});
test("rejects when byte_size is not a number", () => {
  expect(isHistoryEntry({ ...valid, byte_size: "4096" })).toBe(false);
});
test("rejects non-objects", () => {
  expect(isHistoryEntry(null)).toBe(false);
  expect(isHistoryEntry("x")).toBe(false);
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
        invoke: () => Promise.resolve([
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
