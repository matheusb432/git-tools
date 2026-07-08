import { TEST_IDS } from "../../../frontend/viewer/shared/testids/index";

function byTestId(testId: string): string {
  return `[data-testid="${testId}"]`;
}

export const selectors = {
  tabs: byTestId(TEST_IDS.tabs.tab),
  tabsStrip: byTestId(TEST_IDS.tabs.strip),
  historyPanel: byTestId(TEST_IDS.historyPanel.root),
  diffRoot: byTestId(TEST_IDS.diffView.root),
  fileTree: byTestId(TEST_IDS.diffView.fileTree),
  row: byTestId(TEST_IDS.diffView.row),
  rowWindow: byTestId(TEST_IDS.diffView.rowWindow),
  layoutToggle: byTestId(TEST_IDS.diffView.layoutToggle),
  fullToggle: byTestId(TEST_IDS.diffView.fullToggle),
  refreshButton: byTestId(TEST_IDS.diffView.refresh),
  splitLayoutButton: byTestId(TEST_IDS.diffView.layoutSplit),
  unifiedLayoutButton: byTestId(TEST_IDS.diffView.layoutUnified),
  compactButton: byTestId(TEST_IDS.diffView.densityCompact),
  fullButton: byTestId(TEST_IDS.diffView.densityFull),
} as const;

export async function selectorText(selector: string): Promise<string> {
  return browser.execute((value) => {
    const node = document.querySelector(value);
    return node?.textContent?.trim() ?? "";
  }, selector);
}

export async function rowDomCount(): Promise<number> {
  return browser.execute((diffRootSelector: string, rowSelector: string) => {
    const diffRoot = Array.from(document.querySelectorAll(diffRootSelector)).find((candidate) =>
      candidate instanceof HTMLElement && candidate.offsetParent !== null
    );
    return diffRoot instanceof HTMLElement ? diffRoot.querySelectorAll(rowSelector).length : 0;
  }, selectors.diffRoot, selectors.row);
}
