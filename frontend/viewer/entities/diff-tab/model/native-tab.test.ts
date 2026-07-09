import { expect, it, test } from "bun:test";
import type { Recipe, TabMeta } from "@/shared/api";
import {
  beginNativeTabRetry,
  clearTabFreshness,
  markBatchFresh,
  nativeOpeningTab,
  nativeReadyTab,
  nativeRefreshingTab,
  nativeRefreshSucceeded,
  nativeTabError,
  type NativeTab,
} from "./native-tab";

const meta: TabMeta = {
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

test("native tab starts opening and becomes ready with metadata", () => {
  const opening = nativeOpeningTab("local-1", recipe, "batch-1");
  const ready = nativeReadyTab(opening, { tab_id: 7, meta });
  expect(ready.kind).toBe("native");
  expect(ready.lifecycle.state).toBe("ready");
  expect(ready.tabId).toBe(7);
});

test("refresh keeps stale metadata visible until replacement arrives", () => {
  const ready = nativeReadyTab(nativeOpeningTab("local-1", recipe, "batch-1"), { tab_id: 7, meta });
  const refreshing = nativeRefreshingTab(ready);
  expect(refreshing.lifecycle.state).toBe("refreshing");
  expect(refreshing.meta).toBe(meta);
  const replaced = nativeRefreshSucceeded(refreshing, { ...meta, batch_id: "batch-2" });
  expect(replaced.meta?.batch_id).toBe("batch-2");
  expect(replaced.freshness.state).toBe("fresh");
});

function nativeReady(repoName: string, batchId: string): NativeTab {
  return nativeReadyTab(nativeOpeningTab(`local-${repoName}`, recipe, batchId), {
    tab_id: 1,
    meta: { ...meta, batch_id: batchId, repo_name: repoName },
  });
}

it("marks only the latest batch fresh, clears on focus", () => {
  const tabs = [nativeReady("A", "B1"), nativeReady("B", "B2"), nativeReady("C", "B2")];
  const marked = markBatchFresh(tabs, "B2");
  expect(marked.map((t) => t.isNew)).toEqual([false, true, true]);
  const cleared = clearTabFreshness(marked, 1);
  expect(cleared[1]!.isNew).toBe(false);
  expect(cleared[2]!.isNew).toBe(true);
});

test("retrying an initial-open error tab re-enters opening and requests a reopen", () => {
  const failed = nativeTabError(nativeOpeningTab("local-1", recipe, "batch-1"), "boom");

  expect(beginNativeTabRetry(failed)).toEqual({
    kind: "open",
    tab: {
      ...failed,
      lifecycle: { state: "opening" },
    },
  });
});
