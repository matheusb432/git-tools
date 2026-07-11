import { expect, test } from "bun:test";
import type { LiveViewDto } from "@/shared/api";
import { brokenSourceFromRejection, liveTabFromDto } from "./live-tab";

const dto: LiveViewDto = {
  source_kind: "LocalRepo",
  source_value: "/repos/gt",
  display_name: "gt",
  created_at: "2026-07-09T00:00:00Z",
  last_opened_at: null,
};

test("liveTabFromDto maps a persisted row to an unopened durable shell", () => {
  const tab = liveTabFromDto(dto);

  expect(tab.live).toEqual({
    source: { kind: "LocalRepo", value: "/repos/gt" },
    displayName: "gt",
    needsActivation: true,
    brokenSource: null,
  });
  expect(tab.tabId).toBeNull();
  expect(tab.failure).toBeNull();
  expect(tab.recipe).toEqual({
    source: { kind: "LocalRepo", value: "/repos/gt" },
    op: { op: "diff", target: { target: "unpushed" } },
  });
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
