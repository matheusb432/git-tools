import { describe, expect, test } from "vitest";
import { planFileVisibility } from "./file-filter";

function fileWith(path: string, commits: string): Element {
  const el = document.createElement("details");
  el.setAttribute("data-path", path);
  el.setAttribute("data-commits", commits);
  return el;
}

describe("planFileVisibility", () => {
  const files = [fileWith("src/main.rs", "abc def"), fileWith("docs/README.md", "def"), fileWith("src/lib.rs", "")];

  test("no selection and no filter shows everything", () => {
    expect(planFileVisibility(files, { filterText: "", activeShas: null })).toEqual([false, false, false]);
  });

  test("a commit selection hides files without a selected sha", () => {
    expect(planFileVisibility(files, { filterText: "", activeShas: ["abc"] })).toEqual([false, true, true]);
  });

  test("the lowered filter matches against the lowercased path", () => {
    expect(planFileVisibility(files, { filterText: "readme", activeShas: null })).toEqual([true, false, true]);
  });

  test("a file must survive both filters to stay visible", () => {
    expect(planFileVisibility(files, { filterText: "src", activeShas: ["def"] })).toEqual([false, true, true]);
  });
});
