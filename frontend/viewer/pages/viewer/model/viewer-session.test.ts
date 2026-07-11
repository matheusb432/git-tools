import { expect, test } from "bun:test";
import { recipe, tabMeta } from "@/shared/test";
import { createViewerSessionStore, openedTabOutcome, sortTabShellsByMeta, type TabShell } from "./viewer-session";

function opening(localId: string, batchId = "batch-1"): TabShell {
  return {
    localId,
    recipe,
    batchId,
    tabId: null,
    isNew: false,
    failure: null,
    live: null,
  };
}

test("late open returns the backend tab id for cleanup instead of restoring a closed shell", () => {
  expect(openedTabOutcome([], { localId: "local-1", tabId: 9 })).toEqual({ kind: "orphaned", tabId: 9 });
  expect(openedTabOutcome([opening("local-1")], { localId: "local-1", tabId: 9 })).toEqual({ kind: "apply" });
});

test("late open recognizes a reused backend id still owned by a surviving shell", () => {
  expect(openedTabOutcome([{ ...opening("survivor"), tabId: 9 }], { localId: "closed", tabId: 9 })).toEqual({
    kind: "alreadyOwned",
    tabId: 9,
  });
});

test("focus is identity-based and clears only the focused freshness cue", () => {
  const store = createViewerSessionStore();
  store.trigger["tab.openRequested"]({ tab: { ...opening("a"), isNew: true } });
  store.trigger["tab.openRequested"]({ tab: { ...opening("b"), isNew: true } });
  store.trigger["tab.focused"]({ localId: "b" });
  const context = store.getSnapshot().context;
  expect(context.activeLocalId).toBe("b");
  expect(context.tabs.map((tab) => tab.isNew)).toEqual([true, false]);
});

test("restored live shells dedupe by source identity", () => {
  const store = createViewerSessionStore();
  const live = {
    source: { kind: "LocalRepo" as const, value: "/repo" },
    displayName: "repo",
    needsActivation: true,
    brokenSource: null,
  };
  store.trigger["liveViews.restored"]({
    tabs: [
      { ...opening("live:/repo"), live },
      { ...opening("duplicate"), live },
    ],
  });
  expect(store.getSnapshot().context.tabs).toHaveLength(1);
});

test("opening resolves by local identity and removes an older shell for the reused backend tab", () => {
  const store = createViewerSessionStore();
  store.trigger["tab.openRequested"]({ tab: { ...opening("old"), tabId: 9 } });
  store.trigger["tab.openRequested"]({ tab: opening("new", "batch-2") });

  store.trigger["tab.opened"]({ localId: "new", tabId: 9, batchId: "batch-2" });

  expect(store.getSnapshot().context.tabs).toEqual([{ ...opening("new", "batch-2"), tabId: 9 }]);
});

test("a background open completion preserves the user's newer focus", () => {
  const store = createViewerSessionStore();
  store.trigger["tab.openRequested"]({ tab: opening("opening") });
  store.trigger["tab.openRequested"]({ tab: opening("focused") });

  store.trigger["tab.opened"]({ localId: "opening", tabId: 9, batchId: "batch-1" });

  expect(store.getSnapshot().context.activeLocalId).toBe("focused");
});

test("backend id reuse transfers focus only when the focused duplicate is removed", () => {
  const store = createViewerSessionStore();
  store.trigger["tab.openRequested"]({ tab: { ...opening("old"), tabId: 9 } });
  store.trigger["tab.openRequested"]({ tab: opening("replacement", "batch-2") });
  store.trigger["tab.focused"]({ localId: "old" });

  store.trigger["tab.opened"]({ localId: "replacement", tabId: 9, batchId: "batch-2" });

  expect(store.getSnapshot().context.tabs.map((tab) => tab.localId)).toEqual(["replacement"]);
  expect(store.getSnapshot().context.activeLocalId).toBe("replacement");
});

test("close transitions preserve active identity without depending on array positions", () => {
  const store = createViewerSessionStore();
  store.trigger["tab.openRequested"]({ tab: opening("a") });
  store.trigger["tab.openRequested"]({ tab: opening("b") });
  store.trigger["tab.openRequested"]({ tab: opening("c") });
  store.trigger["tab.focused"]({ localId: "b" });

  store.trigger["tab.closed"]({ localId: "a" });
  expect(store.getSnapshot().context.activeLocalId).toBe("b");
  store.trigger["tab.closed"]({ localId: "b" });

  expect(store.getSnapshot().context.tabs.map((tab) => tab.localId)).toEqual(["c"]);
  expect(store.getSnapshot().context.activeLocalId).toBe("c");
});

test("durable failures and live activation stay on their owning shell", () => {
  const store = createViewerSessionStore();
  const live = {
    source: { kind: "LocalRepo" as const, value: "/repo" },
    displayName: "repo",
    needsActivation: true,
    brokenSource: null,
  };
  store.trigger["tab.openRequested"]({ tab: { ...opening("live"), live } });
  store.trigger["liveView.activated"]({ localId: "live" });
  store.trigger["liveView.broken"]({
    localId: "live",
    brokenSource: { code: "DirNotFound", reason: "gone" },
  });

  expect(store.getSnapshot().context.tabs[0]).toMatchObject({
    localId: "live",
    failure: "gone",
    live: { needsActivation: false, brokenSource: { code: "DirNotFound", reason: "gone" } },
  });
});

test("shell sorting reads cached metadata without copying it into session state", () => {
  const older = { ...opening("older", "batch-older"), tabId: 7 };
  const newer = { ...opening("newer", "batch-newer"), tabId: 8 };
  const metadata = new Map([
    [7, { ...tabMeta, tab_id: 7, commits: [{ ...tabMeta.commits[0]!, iso: "2026-07-09T00:00:00Z" }] }],
    [8, { ...tabMeta, tab_id: 8, commits: [{ ...tabMeta.commits[0]!, iso: "2026-07-10T00:00:00Z" }] }],
  ]);

  expect(sortTabShellsByMeta([older, newer], metadata).map((tab) => tab.localId)).toEqual(["newer", "older"]);
});
