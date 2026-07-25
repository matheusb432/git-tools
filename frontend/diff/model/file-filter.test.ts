import { describe, expect, test } from "vitest";
import { type FileFacts, planFileVisibility } from "./file-filter";

describe("planFileVisibility", () => {
  const files: readonly FileFacts[] = [
    { pathLower: "src/main.rs", commitShas: ["abc", "def"] },
    { pathLower: "docs/readme.md", commitShas: ["def"] },
    { pathLower: "src/lib.rs", commitShas: [] },
  ];

  test("a commit selection hides files without a selected sha", () => {
    expect(planFileVisibility(files, { filterText: "", activeShas: new Set(["abc"]) })).toEqual([false, true, true]);
  });

  test("the filter text matches anywhere in the path", () => {
    expect(planFileVisibility(files, { filterText: "readme", activeShas: null })).toEqual([true, false, true]);
  });

  test("a file must survive both filters to stay visible", () => {
    expect(planFileVisibility(files, { filterText: "src", activeShas: new Set(["def"]) })).toEqual([false, true, true]);
  });
});
