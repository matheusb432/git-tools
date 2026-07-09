import { expect, test } from "bun:test";
import type { LiveViewDto } from "@/shared/api";
import { nativeOpeningTab, type ViewerTab } from "./native-tab";
import { brokenSourceFromRejection, dedupeLiveTabBySource, isLiveTab, liveTabFromDto } from "./live-tab";

const dto: LiveViewDto = {
  source_kind: "LocalRepo",
  source_value: "/repos/gt",
  display_name: "gt",
  created_at: "2026-07-09T00:00:00Z",
  last_opened_at: null,
};

test("liveTabFromDto maps a persisted row to an unopened live tab", () => {
  const tab = liveTabFromDto(dto);

  expect(tab.kind).toBe("native");
  expect(tab.live).toBe(true);
  expect(tab.source).toEqual({ kind: "LocalRepo", value: "/repos/gt" });
  expect(tab.displayName).toBe("gt");
  expect(tab.brokenSource).toBeUndefined();
  expect(tab.tabId).toBeNull();
  expect(tab.lifecycle).toEqual({ state: "opening" });
  expect(tab.recipe).toEqual({
    source: { kind: "LocalRepo", value: "/repos/gt" },
    op: { op: "diff", target: { target: "unpushed" } },
  });
});

test("isLiveTab narrows native tabs carrying the live discriminant", () => {
  const live = liveTabFromDto(dto);
  const plainNative = nativeOpeningTab("local-1", live.recipe, "batch-1");

  expect(isLiveTab(live)).toBe(true);
  expect(isLiveTab(plainNative)).toBe(false);
});

test("dedupeLiveTabBySource returns the existing tab's index on a source match", () => {
  const existing = liveTabFromDto(dto);
  const other = nativeOpeningTab("local-1", existing.recipe, "batch-1");
  const tabs: readonly ViewerTab[] = [other, existing];

  expect(dedupeLiveTabBySource(tabs, { kind: "LocalRepo", value: "/repos/gt" })).toBe(1);
  expect(dedupeLiveTabBySource(tabs, { kind: "LocalRepo", value: "/repos/other" })).toBeNull();
});

test("brokenSourceFromRejection maps known codes and rejects unknown ones", () => {
  expect(brokenSourceFromRejection("DirNotFound", "The git repo's directory at `/gone` was not found.")).toEqual({
    code: "DirNotFound",
    reason: "The git repo's directory at `/gone` was not found.",
  });
  expect(brokenSourceFromRejection("DirNotGitRepo", "Not a git repository.")).toEqual({
    code: "DirNotGitRepo",
    reason: "Not a git repository.",
  });
  expect(brokenSourceFromRejection("SomethingElse", "unused")).toBeNull();
});
