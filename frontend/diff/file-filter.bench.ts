import "../test/dom-stub";
import { bench, describe } from "vitest";
import { planFileVisibility } from "./file-filter";

// Models the hot per-keystroke and per-card-click pass of a large diff: every
// visibility recomputation scans all files against the active filters.
const COMMIT_POOL = Array.from({ length: 40 }, (_, index) => `sha${index.toString(16).padStart(7, "0")}`);

function syntheticFiles(count: number): Element[] {
  return Array.from({ length: count }, (_, index) => {
    const file = document.createElement("details");
    file.setAttribute("data-path", `crates/module-${index % 12}/src/deeply/nested/file-${index}.rs`);
    file.setAttribute(
      "data-commits",
      [index % 40, (index * 7) % 40, (index * 13) % 40].map((commit) => COMMIT_POOL[commit]).join(" "),
    );
    return file;
  });
}

const files = syntheticFiles(500);
const selection = COMMIT_POOL.filter((_, index) => index % 13 === 3);

describe("planFileVisibility over 500 files", () => {
  bench("name-filter keystroke", () => {
    planFileVisibility(files, { filterText: "nested/file-1", activeShas: null });
  });

  bench("commit-selection click", () => {
    planFileVisibility(files, { filterText: "", activeShas: selection });
  });

  bench("combined filters", () => {
    planFileVisibility(files, { filterText: "module-7", activeShas: selection });
  });
});
