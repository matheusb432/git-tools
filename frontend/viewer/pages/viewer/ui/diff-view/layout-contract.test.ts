import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

function componentSource(name: string): string {
  return readFileSync(join(import.meta.dir, name), "utf8");
}

test("titlebar renders repository, branch, and upstream metadata", () => {
  const toolbar = componentSource("DiffToolbar.svelte");

  expect(toolbar).toContain("data-testid={TEST_IDS.diffView.titlebar}");
  expect(toolbar).toMatch(/title=\{meta\.repo_root\}>\s*\{meta\.repo_name\}/);
  expect(toolbar).toContain("title={meta.branch}");
  expect(toolbar).toContain("{meta.branch}");
  expect(toolbar).toContain("title={meta.upstream}");
  expect(toolbar).toContain("{meta.upstream}");
});

test("desktop diff shell exposes its layout handles", () => {
  expect(componentSource("FileTree.svelte")).toContain("data-testid={TEST_IDS.diffView.fileTree}");
  expect(componentSource("CommitShelf.svelte")).toContain("data-testid={TEST_IDS.diffView.commitShelf}");
  expect(componentSource("DiffKeybar.svelte")).toContain("data-testid={TEST_IDS.diffView.keybar}");
});

test("virtual file list exposes its list and mounted panel handles", () => {
  const fileList = componentSource("VirtualFileList.svelte");

  expect(fileList).toContain("data-testid={TEST_IDS.diffView.fileList}");
  expect(fileList).toContain("data-testid={TEST_IDS.diffView.filePanelMount}");
  expect(fileList).toContain("useAnimationFrameWithResizeObserver: true");
});
