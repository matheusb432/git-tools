import { expect, test } from "bun:test";
import { TEST_IDS } from "./index";

test("diff view exposes stable toggle item handles for perf automation", () => {
  expect(TEST_IDS.diffView.layoutUnified).toBe("diff-view.layout-unified");
  expect(TEST_IDS.diffView.layoutSplit).toBe("diff-view.layout-split");
  expect(TEST_IDS.diffView.densityCompact).toBe("diff-view.density-compact");
  expect(TEST_IDS.diffView.densityFull).toBe("diff-view.density-full");
});
