import { expect, test } from "bun:test";
import type { Recipe, TabMeta } from "@/shared/api";
import {
  applyOpenedNativeTab,
  closeAllViewerTabs,
  closeOtherViewerTabs,
  closeViewerTabState,
  nativeOpeningTab,
  nativeReadyTab,
  sortViewerTabsByTime,
  type ArtifactTab,
  type ViewerTabState,
} from "..";

const recipe: Recipe = {
  source: { kind: "LocalRepo", value: "/repo" },
  op: { op: "diff", target: { target: "unpushed" } },
};
const meta: TabMeta = {
  tab_id: 7,
  batch_id: "batch-1",
  title: "Native diff",
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
      body: "",
      date: "2026-07-08",
      iso: "2026-07-08T00:00:00Z",
      parents: ["parent-1"],
      members: [],
      is_merge: false,
    },
  ],
  files: [],
};

const artifact = (label: string, committedAt: string): ArtifactTab => ({
  kind: "artifact",
  url: `diff://repo/${label}`,
  label,
  committedAt,
});
const native = (localId: string, batchId = "batch-1") =>
  nativeReadyTab(nativeOpeningTab(localId, recipe, batchId), {
    tab_id: 7,
    meta: { ...meta, batch_id: batchId },
  });
const viewerState = (tabs: ViewerTabState["tabs"], active: number): ViewerTabState => ({
  tabs,
  active,
  showHistory: false,
});

test("closing a native tab keeps active index valid", () => {
  const state = closeViewerTabState(viewerState([artifact("a", "2026-07-07T00:00:00Z"), native("local-1")], 1), 1);
  expect(state.tabs).toEqual([artifact("a", "2026-07-07T00:00:00Z")]);
  expect(state.active).toBe(0);
});

test("closeOtherViewerTabs keeps only the selected native tab", () => {
  const kept = native("local-2");
  const state = closeOtherViewerTabs(
    viewerState([artifact("a", "2026-07-07T00:00:00Z"), kept, artifact("b", "2026-07-06T00:00:00Z")], 0),
    1,
  );
  expect(state.tabs).toEqual([kept]);
  expect(state.active).toBe(0);
});

test("closeAllViewerTabs empties mixed tabs", () => {
  const state = closeAllViewerTabs(viewerState([artifact("a", "2026-07-07T00:00:00Z"), native("local-1")], 1));
  expect(state.tabs).toEqual([]);
  expect(state.active).toBe(0);
});

test("applyOpenedNativeTab closes backend tabs that resolve after the opening shell was closed", () => {
  const opened = { tab_id: 11, meta: { ...meta, tab_id: 11 } };
  const state = viewerState([artifact("a", "2026-07-07T00:00:00Z")], 0);

  expect(applyOpenedNativeTab(state, "local-1", opened)).toEqual({
    next: state,
    openedTabIdToClose: 11,
  });
});

test("sortViewerTabsByTime orders artifact tabs by committedAt and native tabs by latest commit iso", () => {
  const olderArtifact = artifact("artifact-a", "2026-07-07T00:00:00Z");
  const newerArtifact = artifact("artifact-b", "2026-07-08T12:00:00Z");
  const newerNative = native("local-1", "batch-2");
  const olderNative = nativeReadyTab(nativeOpeningTab("local-2", recipe, "batch-0"), {
    tab_id: 8,
    meta: {
      ...meta,
      tab_id: 8,
      batch_id: "batch-0",
      commits: [{ ...meta.commits[0]!, iso: "2026-07-06T12:00:00Z" }],
    },
  });

  const sorted = sortViewerTabsByTime([olderArtifact, olderNative, newerNative, newerArtifact]);
  expect(sorted).toEqual([newerArtifact, newerNative, olderArtifact, olderNative]);
});

test("sortViewerTabsByTime falls back to batchId and stays stable on ties", () => {
  const artifactA = artifact("artifact-a", "2026-07-08T00:00:00Z");
  const artifactB = artifact("artifact-b", "2026-07-08T00:00:00Z");
  const nativeA = nativeReadyTab(nativeOpeningTab("local-a", recipe, "2026-07-08T00:00:00Z"), {
    tab_id: 9,
    meta: { ...meta, tab_id: 9, batch_id: "2026-07-08T00:00:00Z", commits: [] },
  });
  const nativeB = nativeReadyTab(nativeOpeningTab("local-b", recipe, "2026-07-08T00:00:00Z"), {
    tab_id: 10,
    meta: { ...meta, tab_id: 10, batch_id: "2026-07-08T00:00:00Z", commits: [] },
  });

  const sorted = sortViewerTabsByTime([artifactA, artifactB, nativeA, nativeB]);
  expect(sorted).toEqual([artifactA, artifactB, nativeA, nativeB]);
});
