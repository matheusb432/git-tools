import assert from "node:assert/strict";
import { loadFixtureManifest, type FixtureEntry } from "../src/fixtures";
import { rowDomCount, selectors, selectorText } from "../src/handles";
import {
  activateLiveTabBySource,
  activateNativeRepo,
  openNativeRecipe,
  openRecipeBatch,
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
  await waitForVisibleRows(selectors.rowWindow, selectors.row, 1, 20_000, "native diff rows never became visible");
}

async function elementCount(selector: string): Promise<number> {
  const elements = await $$(selector);
  return elements.length;
}

async function visibleElementCount(selector: string): Promise<number> {
  return browser.execute(
    (value: string) =>
      Array.from(document.querySelectorAll(value)).filter(
        (candidate) => candidate instanceof HTMLElement && candidate.offsetParent !== null,
      ).length,
    selector,
  );
}

async function waitForDiffText(included: readonly string[], excluded: readonly string[] = []): Promise<void> {
  await browser.waitUntil(
    async () => {
      const text = await selectorText(selectors.diffRoot);
      return included.every((value) => text.includes(value)) && excluded.every((value) => !text.includes(value));
    },
    { timeout: 20_000, interval: 50, timeoutMsg: `diff text did not settle to include ${included.join(", ")}` },
  );
}

async function waitForToggleState(selector: string, state: "on" | "off"): Promise<void> {
  await browser.waitUntil(async () => (await $(selector).getAttribute("data-state")) === state, {
    timeout: 10_000,
    interval: 50,
    timeoutMsg: `${selector} never reached toggle state ${state}`,
  });
}

/** Waits until the visible row window's signature holds steady across two consecutive reads,
 * then returns it. Virtualized row pages fetch lazily, so `scrollHeight`/the first rows keep
 * shifting for a beat after `waitForRows` — a scenario asserting "nothing changed without a
 * refresh" must baseline against the *settled* signature, not the mid-settle one. */
async function stableRowSignature(timeoutMs = 8_000): Promise<string> {
  let previous = await rowWindowSignature();
  let signature = previous;
  await browser.waitUntil(
    async () => {
      await browser.pause(150);
      signature = await rowWindowSignature();
      const stable = signature === previous;
      previous = signature;
      return stable;
    },
    { timeout: timeoutMs, interval: 50, timeoutMsg: "row window signature never stabilized" },
  );
  return signature;
}

/** Restored live views arrive asynchronously off `restoreOnMount` — poll the test-API
 * snapshot (an auto-retrying assertion, not a sleep) rather than assuming they're already
 * pushed by the time a scenario runs. */
async function waitForLiveTab(sourceValue: string, timeoutMs = 20_000): Promise<void> {
  await browser.waitUntil(
    async () => {
      const snapshot = await viewerSnapshot();
      return snapshot.tabs.some(
        (tab) => tab.kind === "native" && tab.live === true && tab.source?.value === sourceValue,
      );
    },
    { timeout: timeoutMs, interval: 50, timeoutMsg: `live tab for source ${sourceValue} was never restored` },
  );
}

/** On cold start `restoreOnMount` auto-computes the first restored live tab, and that background
 * compute focuses that tab when it settles — a one-time focus-steal. Waiting for the tab to reach
 * a settled lifecycle (`ready` or `error`) in the `before` hook makes that steal happen up front,
 * so later scenarios that focus a tab own the last focus change and don't race the restore. */
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

/** Waits until the given native repo is the active tab (its own focus change has landed and
 * won). Paired with `waitForRows` before asserting, this makes a scenario robust to any
 * still-in-flight restore focus-steal. */
async function waitForActiveRepo(repoName: string, timeoutMs = 20_000): Promise<void> {
  await browser.waitUntil(async () => (await viewerSnapshot()).activeTab?.repoName === repoName, {
    timeout: timeoutMs,
    interval: 50,
    timeoutMsg: `repo ${repoName} never became the active tab`,
  });
}

describe("native diff functional scenarios", () => {
  const small = fixture("small");
  const twoSmallFiles = fixture("two-small-files");
  const large = fixture("large-file");
  const many = fixture("many-files");

  before(async () => {
    await $(selectors.tabsStrip).waitForDisplayed({ timeout: 30_000 });
    // Let the cold-start restore of the healthy live view finish its one-time auto-compute (and
    // the focus-steal that rides it) before any scenario drives the strip.
    await waitForLiveTabSettled(manifest.liveViews.healthy.sourceValue);
  });

  it("opens a native tab for a recipe with visible rows", async () => {
    await openNativeRecipe(small.recipe, "func-open-1");
    await waitForActiveRepo(small.repoName);
    await waitForRows();

    const snapshot = await viewerSnapshot();
    assert.equal(snapshot.activeTab?.kind, "native", "expected the newly opened tab to be native");
    assert.equal(snapshot.activeTab?.lifecycle, "ready", "expected the native tab to reach the ready lifecycle");
    assert((await rowDomCount()) > 0, "expected native diff rows to be visible");
  });

  it("keeps two small files settled through settings, tab, filter, and fold transitions", async () => {
    const secondaryFile = twoSmallFiles.secondaryFile;
    const secondaryRowText = twoSmallFiles.expectedSecondaryRowText;
    assert(secondaryFile !== null, "two-small-files fixture must expose its secondary filename");
    assert(secondaryRowText !== null, "two-small-files fixture must expose its secondary row text");

    await openNativeRecipe(twoSmallFiles.recipe, "func-two-small-files");
    await waitForActiveRepo(twoSmallFiles.repoName);
    await waitForDiffText([twoSmallFiles.expectedPrimaryRowText, secondaryRowText]);
    assert.equal(await visibleElementCount(selectors.rowSkeleton), 0, "two settled files must show no row skeletons");
    const fileTreeText = await selectorText(selectors.fileTree);
    assert(fileTreeText.includes(twoSmallFiles.primaryFile), `missing ${twoSmallFiles.primaryFile} in the file tree`);
    assert(fileTreeText.includes(secondaryFile), `missing ${secondaryFile} in the file tree`);
    await waitForToggleState(selectors.splitLayoutButton, "on");
    await waitForToggleState(selectors.fullButton, "on");

    await $(selectors.unifiedLayoutButton).click();
    await waitForToggleState(selectors.unifiedLayoutButton, "on");
    await $(selectors.compactButton).click();
    await waitForToggleState(selectors.compactButton, "on");
    await waitForDiffText([twoSmallFiles.expectedPrimaryRowText, secondaryRowText]);
    assert.equal(
      await visibleElementCount(selectors.rowSkeleton),
      0,
      "unified compact rows must settle without skeletons",
    );

    await $(selectors.splitLayoutButton).click();
    await waitForToggleState(selectors.splitLayoutButton, "on");
    await $(selectors.fullButton).click();
    await waitForToggleState(selectors.fullButton, "on");
    await waitForDiffText([twoSmallFiles.expectedPrimaryRowText, secondaryRowText]);

    await activateNativeRepo(small.repoName);
    await waitForActiveRepo(small.repoName);
    await activateNativeRepo(twoSmallFiles.repoName);
    await waitForActiveRepo(twoSmallFiles.repoName);
    await waitForDiffText([twoSmallFiles.expectedPrimaryRowText, secondaryRowText]);

    await $(selectors.filter).setValue(twoSmallFiles.primaryFile);
    await browser.waitUntil(
      async () => {
        const text = await selectorText(selectors.fileTree);
        return text.includes(twoSmallFiles.primaryFile) && !text.includes(secondaryFile);
      },
      { timeout: 10_000, interval: 50, timeoutMsg: "file filter never narrowed to the primary file" },
    );
    await waitForDiffText([twoSmallFiles.expectedPrimaryRowText], [secondaryRowText]);

    await $(selectors.filter).clearValue();
    await browser.execute((selector: string) => {
      const input = document.querySelector(selector);
      if (input instanceof HTMLInputElement) input.dispatchEvent(new InputEvent("input", { bubbles: true }));
    }, selectors.filter);
    await waitForDiffText([twoSmallFiles.expectedPrimaryRowText, secondaryRowText]);

    const foldAll = await $("button=Fold all");
    await foldAll.click();
    await $("button=Expand all").waitForDisplayed({ timeout: 10_000 });
    assert.equal(await visibleElementCount(selectors.rowSkeleton), 0, "collapsed files must not retain row skeletons");
    await $("button=Expand all").click();
    await $("button=Fold all").waitForDisplayed({ timeout: 10_000 });
    await waitForDiffText([twoSmallFiles.expectedPrimaryRowText, secondaryRowText]);
    assert.equal(await visibleElementCount(selectors.rowSkeleton), 0, "expanded files must settle without skeletons");

    const snapshot = await viewerSnapshot();
    assert.deepEqual(snapshot.browserErrors, [], "two-file transitions must not produce uncaught browser errors");
  });

  it("does not duplicate a native tab for the same recipe source", async () => {
    await openNativeRecipe(small.recipe, "func-dedupe-1");
    await waitForRows();
    await openNativeRecipe(small.recipe, "func-dedupe-2");
    await waitForRows();

    const snapshot = await viewerSnapshot();
    const matches = snapshot.tabs.filter((tab) => tab.kind === "native" && tab.repoName === small.repoName);
    assert.equal(matches.length, 1, `expected exactly one native tab for ${small.repoName}, got ${matches.length}`);
    assert.equal(
      matches[0]?.batchId,
      "func-dedupe-2",
      "expected the reused tab's batch id to update to the latest open",
    );
  });

  it("marks a batch's tabs fresh with a toast, clearing the cue on focus", async () => {
    await openRecipeBatch({ batchId: "func-all-1", recipes: [large.recipe, many.recipe] });
    await waitForRows();

    await $(selectors.toastItem).waitForDisplayed({ timeout: 5_000 });
    const snapshot = await viewerSnapshot();
    assert(
      snapshot.toasts.some((toast) => toast.message === "Opened 2 diffs — gtl diff (2 repos)"),
      `expected a batch-open toast; got ${JSON.stringify(snapshot.toasts.map((toast) => toast.message))}`,
    );
    assert.equal(await elementCount(selectors.freshDot), 2, "expected both batch tabs to show the fresh-dot cue");

    await activateNativeRepo(large.repoName);
    await browser.waitUntil(async () => (await elementCount(selectors.freshDot)) === 1, {
      timeout: 5_000,
      timeoutMsg: "focusing a batch tab never cleared its fresh-dot cue",
    });
  });

  it("refreshes a native tab only on an explicit click, keeping stale rows until replacement", async () => {
    await activateNativeRepo(small.repoName);
    await waitForRows();
    const signature = await stableRowSignature();

    // * Proving the *absence* of a background refresh over a bounded window — there is no
    // * condition to poll for here, so a deliberate pause (not a retry loop) is the right tool.
    await browser.pause(500);
    assert.equal(await rowWindowSignature(), signature, "native rows changed without a refresh click");

    assert((await rowDomCount()) > 0, "expected rows to exist before refreshing");
    await $(selectors.refreshButton).click();
    assert((await rowDomCount()) > 0, "refresh cleared stale rows before the replacement rows arrived");

    await waitForRows();
  });

  it("surfaces a typed error for a broken saved live-view source", async () => {
    const broken = manifest.liveViews.broken;
    await waitForLiveTab(broken.sourceValue);
    await activateLiveTabBySource(broken.sourceValue);

    await $(selectors.brokenSource).waitForDisplayed({ timeout: 10_000 });
    const message = await selectorText(selectors.brokenSource);
    assert(message.includes("was not found"), `expected the DirNotFound reason, got: ${message}`);

    const snapshot = await viewerSnapshot();
    assert.deepEqual(snapshot.browserErrors, [], "a broken live source must not throw an uncaught browser error");
  });

  it("restores a saved live view on startup", async () => {
    // * Proof level: this asserts restore-from-a-pre-seeded db at session start (the `gtl.db`
    // * row is written by `seedLiveViewFixtures` before the viewer process ever opens it — see
    // * `src/db.ts`). A full mid-session tauri-driver process restart is impractical in this
    // * harness (single-instance app, one webdriver session per spec file); a fresh session
    // * start discovering persisted state is the equivalent proof for `restoreOnMount`.
    const healthy = manifest.liveViews.healthy;
    await waitForLiveTab(healthy.sourceValue);

    const snapshot = await viewerSnapshot();
    const restored = snapshot.tabs.find(
      (tab) => tab.kind === "native" && tab.live === true && tab.source?.value === healthy.sourceValue,
    );
    assert(restored !== undefined, "expected the healthy live view to be restored at session start");
  });
});
