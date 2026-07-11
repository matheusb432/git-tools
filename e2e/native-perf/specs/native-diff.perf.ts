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

/** On cold start `restoreOnMount` auto-computes the first restored live tab, and that background
 * compute focuses that tab when it settles — a one-time focus-steal (see the same guard in
 * `native-diff.functional.ts`). This spec's fixture seed always includes the healthy live view
 * (shared `seed.ts`), so its own cold start races that steal too — wait for it to settle before
 * any timed interaction, or the steal can land mid-measurement and blow a gate or a focus wait. */
async function waitForLiveTabSettled(sourceValue: string, timeoutMs = 20_000): Promise<void> {
  await browser.waitUntil(
    async () => {
      const snapshot = await viewerSnapshot();
      const tab = snapshot.tabs.find(
        (candidate) =>
          candidate.kind === "native" && candidate.live === true && candidate.source?.value === sourceValue,
      );
      return tab !== undefined && (tab.lifecycle === "ready" || tab.lifecycle === "error");
    },
    { timeout: timeoutMs, interval: 50, timeoutMsg: `live tab for source ${sourceValue} never settled its compute` },
  );
}

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
    await waitForVisibleRows(selectors.rowWindow, selectors.row, 1, 20_000, "native diff rows never became visible");
  } catch (error) {
    const snapshot = await viewerSnapshot().catch(() => null);
    const domState = await browser
      .execute(
        (diffRootSelector: string, toolbarSelector: string, fileTreeSelector: string, rowWindowSelector: string) => {
          const diffRoot = document.querySelector(diffRootSelector);
          return {
            bodyText: document.body.textContent?.replace(/\s+/g, " ").trim().slice(0, 400) ?? "",
            hasToolbar: document.querySelector(toolbarSelector) !== null,
            hasFileTree: document.querySelector(fileTreeSelector) !== null,
            hasRowWindow: document.querySelector(rowWindowSelector) !== null,
            diffRootHtml: diffRoot?.innerHTML.slice(0, 800) ?? "",
          };
        },
        selectors.diffRoot,
        selectors.layoutToggle,
        selectors.fileTree,
        selectors.rowWindow,
      )
      .catch(() => null);
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}\nviewer snapshot: ${JSON.stringify(snapshot, null, 2)}\ndom state: ${JSON.stringify(domState, null, 2)}`,
    );
  }
}

async function activateRepo(repoName: string): Promise<void> {
  if ((await viewerSnapshot()).activeTab?.repoName !== repoName) await activateNativeRepo(repoName);
  await browser.waitUntil(async () => (await viewerSnapshot()).activeTab?.repoName === repoName, {
    timeout: 10_000,
    interval: 50,
    timeoutMsg: `repo ${repoName} did not become active after switching tabs`,
  });
  await waitForRows();
}

async function waitForToggleState(selector: string, state: "on" | "off"): Promise<void> {
  await browser.waitUntil(async () => (await $(selector).getAttribute("data-state")) === state, {
    timeout: 10_000,
    interval: 50,
    timeoutMsg: `${selector} never reached toggle state ${state}`,
  });
}

async function normalizeViewerSettings(): Promise<void> {
  if ((await $(selectors.unifiedLayoutButton).getAttribute("data-state")) !== "on") {
    await $(selectors.unifiedLayoutButton).click();
  }
  await waitForToggleState(selectors.unifiedLayoutButton, "on");
  if ((await $(selectors.compactButton).getAttribute("data-state")) !== "on") {
    await $(selectors.compactButton).click();
  }
  await waitForToggleState(selectors.compactButton, "on");
  await waitForRows();
  await browser.waitUntil(async () => (await $$(selectors.rowSkeleton)).length === 0, {
    timeout: 10_000,
    interval: 50,
    timeoutMsg: "row skeletons did not settle after normalizing viewer settings",
  });
}

describe("native diff perf harness", () => {
  it("captures the native diff performance gates", async () => {
    const small = fixture("small");
    const large = fixture("large-file");
    const many = fixture("many-files");

    await $(selectors.tabsStrip).waitForDisplayed({ timeout: 30_000 });
    // Let the cold-start restore of the healthy live view finish its one-time auto-compute (and
    // the focus-steal that rides it) before the timed interactions below.
    await waitForLiveTabSettled(manifest.liveViews.healthy.sourceValue);

    await openNativeRecipe(small.recipe, "perf-settings-warmup");
    await waitForRows();
    await normalizeViewerSettings();
    const activeTab = await $(`${selectors.tabs}[aria-selected="true"]`);
    await activeTab.$("button").click();
    await browser.waitUntil(async () => !(await viewerSnapshot()).tabs.some((tab) => tab.repoName === small.repoName), {
      timeout: 10_000,
      interval: 50,
      timeoutMsg: "settings warmup tab did not close",
    });

    const openMetrics = await openNativeRecipeMeasured(small.recipe, "perf-small");
    await waitForRows();
    const firstRowsAt = await browserNow();

    const shellVisibleMs = openMetrics.shellVisibleAt - openMetrics.startedAt;
    const computeToRowsMs = firstRowsAt - openMetrics.resolvedAt;
    recordTiming({ name: "shell-visible-ms", ms: shellVisibleMs, gateMs: 300 });
    recordTiming({ name: "compute-to-first-rows-ms", ms: computeToRowsMs, gateMs: 200 });
    assert(shellVisibleMs < 300, `shell visible gate failed: ${shellVisibleMs.toFixed(2)}ms`);
    assert(computeToRowsMs < 200, `compute-to-first-rows gate failed: ${computeToRowsMs.toFixed(2)}ms`);

    let rowPageQueryPeak = 0;
    const sampleRowPageQueryPeak = async (): Promise<void> => {
      rowPageQueryPeak = Math.max(rowPageQueryPeak, (await viewerSnapshot()).queryCache.rowPages);
    };

    await openNativeRecipe(large.recipe, "perf-large");
    await browser.waitUntil(async () => (await viewerSnapshot()).activeTab?.repoName === large.repoName, {
      timeout: 20_000,
      interval: 50,
      timeoutMsg: `large fixture ${large.repoName} never became active`,
    });
    await waitForRows();
    await normalizeViewerSettings();
    await sampleRowPageQueryPeak();

    const liveRows = await rowDomCount();
    recordDomCount({ name: "large-file-live-row-nodes", count: liveRows, limit: 500 });
    assert(liveRows <= 500, `expected <= 500 live row nodes, got ${liveRows}`);

    const scrollMetrics = await measureSustainedScroll(selectors.fileList, 1_500);
    recordTiming({
      name: "large-file-scroll-effective-fps",
      value: scrollMetrics.effectiveFps,
      unit: "fps",
      minimum: 60,
    });
    recordTiming({ name: "large-file-scroll-longest-frame-ms", ms: scrollMetrics.longestFrameMs, gateMs: 50 });
    assert(
      scrollMetrics.effectiveFps >= 60,
      `large-file scroll FPS gate failed: ${scrollMetrics.effectiveFps.toFixed(2)}fps`,
    );
    assert(
      scrollMetrics.longestFrameMs <= 50,
      `large-file main-thread block gate failed: ${scrollMetrics.longestFrameMs.toFixed(2)}ms`,
    );
    await sampleRowPageQueryPeak();

    await normalizeViewerSettings();
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
    await sampleRowPageQueryPeak();

    await activateRepo(small.repoName);
    await waitForRows();
    await normalizeViewerSettings();
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
    await sampleRowPageQueryPeak();

    const rowsBeforeRefresh = await rowDomCount();
    assert(rowsBeforeRefresh > 0, "expected stale rows to exist before refreshing");

    await $(selectors.refreshButton).click();
    assert((await rowDomCount()) > 0, "native refresh cleared stale rows before replacement rows arrived");

    await waitForRows();

    await normalizeViewerSettings();
    await openNativeRecipe(many.recipe, "perf-many");
    await browser.waitUntil(async () => (await viewerSnapshot()).activeTab?.repoName === many.repoName, {
      timeout: 20_000,
      interval: 50,
      timeoutMsg: `many-files fixture ${many.repoName} never became active`,
    });
    await waitForRows();
    await normalizeViewerSettings();
    await sampleRowPageQueryPeak();
    assert(
      (await selectorText(selectors.fileTree)).includes(many.primaryFile),
      "many-files fixture did not render its file tree",
    );

    const manyRows = await rowDomCount();
    recordDomCount({ name: "many-files-live-row-nodes", count: manyRows, limit: 500 });
    assert(manyRows <= 500, `expected <= 500 live row nodes after many-files fixture, got ${manyRows}`);

    recordDomCount({ name: "row-page-query-peak", count: rowPageQueryPeak, limit: 256 });
    assert(rowPageQueryPeak <= 256, `expected <= 256 cached row-page queries, got ${rowPageQueryPeak}`);

    await normalizeViewerSettings();
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
