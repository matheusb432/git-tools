import assert from "node:assert/strict";
import { captureScreenshot, recordDomCount, recordTiming } from "../src/evidence";
import { loadFixtureManifest, type FixtureEntry } from "../src/fixtures";
import { rowDomCount, selectors, selectorText } from "../src/handles";
import {
  activateNativeRepo,
  browserNow,
  measureClickFirstInactiveTabUntilTextChanges,
  measureClickUntilRowWindowChanges,
  measureSustainedScroll,
  openNativeRecipe,
  openNativeRecipeMeasured,
  rowWindowSignature,
  viewerSnapshot,
  waitForVisibleRows,
} from "../src/timing";

const manifest = loadFixtureManifest();

function fixture(name: FixtureEntry["name"]): FixtureEntry {
  const match = manifest.fixtures.find((entry) => entry.name === name);
  if (match === undefined) {
    throw new Error(`Missing fixture ${name} in ${process.env.GTL_NATIVE_PERF_FIXTURE_ROOT ?? "unknown root"}`);
  }
  return match;
}

async function waitForRows(): Promise<void> {
  try {
    await waitForVisibleRows(
      selectors.rowWindow,
      selectors.row,
      1,
      20_000,
      "native diff rows never became visible",
    );
  } catch (error) {
    const snapshot = await viewerSnapshot().catch(() => null);
    const domState = await browser.execute((diffRootSelector: string, toolbarSelector: string, fileTreeSelector: string, rowWindowSelector: string) => {
      const diffRoot = document.querySelector(diffRootSelector);
      return {
        bodyText: document.body.textContent?.replace(/\s+/g, " ").trim().slice(0, 400) ?? "",
        hasToolbar: document.querySelector(toolbarSelector) !== null,
        hasFileTree: document.querySelector(fileTreeSelector) !== null,
        hasRowWindow: document.querySelector(rowWindowSelector) !== null,
        diffRootHtml: diffRoot?.innerHTML.slice(0, 800) ?? "",
      };
    }, selectors.diffRoot, selectors.layoutToggle, selectors.fileTree, selectors.rowWindow).catch(() => null);
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}\nviewer snapshot: ${JSON.stringify(snapshot, null, 2)}\ndom state: ${JSON.stringify(domState, null, 2)}`,
    );
  }
}

async function activateRepo(repoName: string): Promise<void> {
  if ((await selectorText(selectors.fileTree)).includes(repoName)) {
    return;
  }

  await activateNativeRepo(repoName);
  await browser.waitUntil(async () => (await selectorText(selectors.fileTree)).includes(repoName), {
    timeout: 10_000,
    interval: 10,
    timeoutMsg: `repo ${repoName} did not become active after switching tabs`,
  });
}

describe("native diff perf harness", () => {
  it("captures the native diff performance gates", async () => {
    const small = fixture("small");
    const large = fixture("large-file");
    const many = fixture("many-files");

    await $(selectors.tabsStrip).waitForDisplayed({ timeout: 30_000 });

    const openMetrics = await openNativeRecipeMeasured(small.recipe, "perf-small");
    await waitForRows();
    const firstRowsAt = await browserNow();

    const shellVisibleMs = openMetrics.shellVisibleAt - openMetrics.startedAt;
    const computeToRowsMs = firstRowsAt - openMetrics.resolvedAt;
    recordTiming({ name: "shell-visible-ms", ms: shellVisibleMs, gateMs: 300 });
    recordTiming({ name: "compute-to-first-rows-ms", ms: computeToRowsMs, gateMs: 200 });
    assert(shellVisibleMs < 300, `shell visible gate failed: ${shellVisibleMs.toFixed(2)}ms`);
    assert(computeToRowsMs < 200, `compute-to-first-rows gate failed: ${computeToRowsMs.toFixed(2)}ms`);

    await openNativeRecipe(large.recipe, "perf-large");
    await browser.waitUntil(async () => (await selectorText(selectors.fileTree)).includes(large.repoName), {
      timeout: 20_000,
      interval: 10,
      timeoutMsg: `large fixture ${large.repoName} never became active`,
    });
    await waitForRows();

    const liveRows = await rowDomCount();
    recordDomCount({ name: "large-file-live-row-nodes", count: liveRows, limit: 500 });
    assert(liveRows <= 500, `expected <= 500 live row nodes, got ${liveRows}`);

    const scrollMetrics = await measureSustainedScroll(selectors.rowWindow, 1_500);
    recordTiming({ name: "large-file-scroll-effective-fps", value: scrollMetrics.effectiveFps, unit: "fps", minimum: 60 });
    recordTiming({ name: "large-file-scroll-longest-frame-ms", ms: scrollMetrics.longestFrameMs, gateMs: 50 });
    assert(
      scrollMetrics.effectiveFps >= 60,
      `large-file scroll FPS gate failed: ${scrollMetrics.effectiveFps.toFixed(2)}fps`,
    );
    assert(
      scrollMetrics.longestFrameMs <= 50,
      `large-file main-thread block gate failed: ${scrollMetrics.longestFrameMs.toFixed(2)}ms`,
    );

    const layoutSignature = await rowWindowSignature();
    const splitToggleMs = await measureClickUntilRowWindowChanges(
      selectors.splitLayoutButton,
      selectors.rowWindow,
      selectors.row,
      layoutSignature,
      10_000,
      "split layout never repainted",
    );
    recordTiming({ name: "toggle-split-ms", ms: splitToggleMs, gateMs: 150 });
    assert(splitToggleMs < 150, `split toggle gate failed: ${splitToggleMs.toFixed(2)}ms`);

    await activateRepo(small.repoName);
    await waitForRows();
    const densitySignature = await rowWindowSignature();
    const fullToggleMs = await measureClickUntilRowWindowChanges(
      selectors.fullButton,
      selectors.rowWindow,
      selectors.row,
      densitySignature,
      10_000,
      "full diff toggle never repainted",
    );
    recordTiming({ name: "toggle-full-ms", ms: fullToggleMs, gateMs: 150 });
    assert(fullToggleMs < 150, `full toggle gate failed: ${fullToggleMs.toFixed(2)}ms`);

    const rowsBeforeRefresh = await rowDomCount();
    assert(rowsBeforeRefresh > 0, "expected stale rows to exist before refreshing");

    await $(selectors.refreshButton).click();
    assert((await rowDomCount()) > 0, "native refresh cleared stale rows before replacement rows arrived");

    await waitForRows();

    await openNativeRecipe(many.recipe, "perf-many");
    await browser.waitUntil(async () => (await selectorText(selectors.fileTree)).includes(many.repoName), {
      timeout: 20_000,
      interval: 10,
      timeoutMsg: `many-files fixture ${many.repoName} never became active`,
    });
    await waitForRows();
    assert((await selectorText(selectors.fileTree)).includes(many.primaryFile), "many-files fixture did not render its file tree");

    const manyRows = await rowDomCount();
    recordDomCount({ name: "many-files-live-row-nodes", count: manyRows, limit: 500 });
    assert(manyRows <= 500, `expected <= 500 live row nodes after many-files fixture, got ${manyRows}`);

    const currentFileTreeText = await selectorText(selectors.fileTree);
    const tabSwitchMs = await measureClickFirstInactiveTabUntilTextChanges(
      selectors.tabs,
      selectors.fileTree,
      currentFileTreeText,
      10_000,
      "native tab switch did not change the file tree",
    );
    recordTiming({ name: "tab-switch-ms", ms: tabSwitchMs, gateMs: 100 });
    assert(tabSwitchMs < 100, `tab switch gate failed: ${tabSwitchMs.toFixed(2)}ms`);
    await captureScreenshot("native-diff-perf-final");
  });
});
