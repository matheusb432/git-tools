import { expect, test } from "bun:test";
import { drainPendingDiffs, listHistory } from "./tauri";

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
