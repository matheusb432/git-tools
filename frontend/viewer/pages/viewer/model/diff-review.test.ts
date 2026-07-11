import { expect, test } from "bun:test";
import { useSelector } from "@xstate/store-svelte";
import { createDiffReviewStore } from "./diff-review";

test("review events update only their focused state", () => {
  const store = createDiffReviewStore();
  store.trigger["filter.changed"]({ value: "src/" });
  store.trigger["commit.toggled"]({ sha: "abc1234" });
  store.trigger["file.selected"]({ fileIdx: 3 });

  const context = store.getSnapshot().context;
  expect(context.filterText).toBe("src/");
  expect(context.focusedCommits).toEqual(new Set(["abc1234"]));
  expect(context.selectedFileIdx).toBe(3);
  expect(context.collapsedFileIdxs.has(3)).toBe(false);
});

test("unrelated transitions preserve non-target Set identity", () => {
  const store = createDiffReviewStore();
  const initial = store.getSnapshot().context;

  store.trigger["filter.changed"]({ value: "src/" });

  const current = store.getSnapshot().context;
  expect(current.focusedCommits).toBe(initial.focusedCommits);
  expect(current.collapsedFileIdxs).toBe(initial.collapsedFileIdxs);
  expect(current.expandedLongRows).toBe(initial.expandedLongRows);
});

test("selector subscribers skip unrelated transitions", () => {
  const store = createDiffReviewStore();
  const focusedCommits = useSelector(store, (snapshot) => snapshot.context.focusedCommits);
  const observed: ReadonlySet<string>[] = [];
  const unsubscribe = focusedCommits.subscribe((value) => observed.push(value));

  store.trigger["filter.changed"]({ value: "src/" });
  expect(observed).toHaveLength(1);

  store.trigger["commit.toggled"]({ sha: "abc1234" });
  expect(observed).toEqual([new Set(), new Set(["abc1234"])]);
  unsubscribe();
});

test("commit focus can be toggled and cleared", () => {
  const store = createDiffReviewStore();
  store.trigger["commit.toggled"]({ sha: "abc1234" });
  store.trigger["commit.toggled"]({ sha: "def5678" });
  store.trigger["commit.toggled"]({ sha: "abc1234" });
  expect(store.getSnapshot().context.focusedCommits).toEqual(new Set(["def5678"]));

  store.trigger["commits.cleared"]();
  expect(store.getSnapshot().context.focusedCommits).toEqual(new Set());
});

test("panel events preserve hidden folds and selected files expand", () => {
  const store = createDiffReviewStore();
  store.trigger["panel.toggled"]({ fileIdx: 9 });
  store.trigger["panels.toggledAll"]({ visibleFileIdxs: [1, 2] });
  expect(store.getSnapshot().context.collapsedFileIdxs).toEqual(new Set([9, 1, 2]));

  store.trigger["panels.toggledAll"]({ visibleFileIdxs: [1, 2] });
  expect(store.getSnapshot().context.collapsedFileIdxs).toEqual(new Set([9]));

  store.trigger["panel.toggled"]({ fileIdx: 3 });
  store.trigger["file.selected"]({ fileIdx: 3 });
  expect(store.getSnapshot().context.collapsedFileIdxs).toEqual(new Set([9]));
});

test("long-row expansion is an explicit toggle event", () => {
  const store = createDiffReviewStore();
  store.trigger["longRow.toggled"]({ rowKey: "u:4" });
  expect(store.getSnapshot().context.expandedLongRows).toEqual(new Set(["u:4"]));

  store.trigger["longRow.toggled"]({ rowKey: "u:4" });
  expect(store.getSnapshot().context.expandedLongRows).toEqual(new Set());
});
