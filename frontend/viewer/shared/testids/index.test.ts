import { expect, test } from "bun:test";
import { TEST_IDS } from "./index";

test("diff view exposes stable interaction and layout handles", () => {
  expect(TEST_IDS.diffView.layoutUnified).toBe("diff-view.layout-unified");
  expect(TEST_IDS.diffView.layoutSplit).toBe("diff-view.layout-split");
  expect(TEST_IDS.diffView.densityCompact).toBe("diff-view.density-compact");
  expect(TEST_IDS.diffView.densityFull).toBe("diff-view.density-full");
  expect(TEST_IDS.diffView.fileTree).toBe("diff-view.file-tree");
  expect(TEST_IDS.diffView.fileList).toBe("diff-view.file-list");
  expect(TEST_IDS.diffView.filePanelMount).toBe("diff-view.file-panel-mount");
  expect(TEST_IDS.diffView.rowSkeleton).toBe("diff-view.row-skeleton");
  expect(TEST_IDS.diffView.commitShelf).toBe("diff-view.commit-shelf");
  expect(TEST_IDS.diffView.keybar).toBe("diff-view.keybar");
});
