import type { OpenRecipes, Recipe } from "../../../frontend/viewer/shared/api/index";
import { selectors } from "./handles";

export type OpenMetrics = {
  readonly startedAt: number;
  readonly shellVisibleAt: number;
  readonly resolvedAt: number;
  readonly batchId: string;
};

type OpenOutcome =
  | { readonly ok: true; readonly metrics: OpenMetrics }
  | { readonly ok: false; readonly error: string };

type ViewerTestApi = {
  readonly openNativeRecipe: (recipe: Recipe, batchId?: string) => Promise<void>;
  readonly activateNativeRepo: (repoName: string) => Promise<boolean>;
  readonly announceBatch: (count: number, commandLabel: string) => void;
  readonly openRecipeBatch: (batch: OpenRecipes) => Promise<void>;
  readonly activateLiveTabBySource: (sourceValue: string) => boolean;
  readonly snapshot: () => ViewerSnapshot;
};

export type ViewerSnapshot = {
  readonly active: number;
  readonly activeTab: {
    readonly kind: "native";
    readonly tabId?: number | null;
    readonly lifecycle?: "opening" | "ready" | "refreshing" | "error";
    readonly repoName?: string | null;
  } | null;
  readonly shellError: string | null;
  readonly browserErrors: readonly string[];
  readonly tabs: readonly {
    readonly kind: "native";
    readonly localId?: string;
    readonly tabId?: number | null;
    readonly batchId?: string;
    readonly lifecycle?: "opening" | "ready" | "refreshing" | "error";
    readonly error?: string;
    readonly repoName?: string | null;
    readonly fileCount?: number | null;
    readonly live?: boolean;
    readonly source?: { readonly kind: string; readonly value: string } | null;
  }[];
  readonly toasts: readonly { readonly id: number; readonly message: string }[];
};

declare global {
  interface Window {
    __GTL_VIEWER_TEST__?: ViewerTestApi;
  }
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isOpenMetrics(value: unknown): value is OpenMetrics {
  if (!isObjectRecord(value)) return false;
  return (
    typeof value.startedAt === "number" &&
    typeof value.shellVisibleAt === "number" &&
    typeof value.resolvedAt === "number" &&
    typeof value.batchId === "string"
  );
}

function isOpenOutcome(value: unknown): value is OpenOutcome {
  if (!isObjectRecord(value) || typeof value.ok !== "boolean") return false;
  if (value.ok) {
    return isOpenMetrics(value.metrics);
  }
  return typeof value.error === "string";
}

export async function browserNow(): Promise<number> {
  return browser.execute(() => window.performance.now());
}

export async function viewerSnapshot(): Promise<ViewerSnapshot> {
  const snapshot = await browser.execute(() => {
    const api = window.__GTL_VIEWER_TEST__;
    if (api === undefined || typeof api.snapshot !== "function") {
      throw new Error("window.__GTL_VIEWER_TEST__.snapshot is unavailable");
    }
    return api.snapshot();
  });

  return snapshot as ViewerSnapshot;
}

export async function waitForSelectorCountAtLeast(
  selector: string,
  minimum: number,
  timeoutMs: number,
  timeoutMessage: string,
): Promise<void> {
  const result = await browser.executeAsync((
    cssSelector: string,
    minCount: number,
    timeout: number,
    message: string,
    done: (value: string | null) => void,
  ) => {
    const deadline = window.performance.now() + timeout;

    const tick = (): void => {
      if (document.querySelectorAll(cssSelector).length >= minCount) {
        done(null);
        return;
      }
      if (window.performance.now() >= deadline) {
        done(message);
        return;
      }
      window.requestAnimationFrame(tick);
    };

    tick();
  }, selector, minimum, timeoutMs, timeoutMessage);

  if (typeof result === "string" && result !== "") {
    throw new Error(result);
  }
}

export async function waitForVisibleRows(
  rowWindowSelector: string,
  rowSelector: string,
  minimum: number,
  timeoutMs: number,
  timeoutMessage: string,
): Promise<void> {
  const result = await browser.executeAsync((
    windowSelector: string,
    cssSelector: string,
    minCount: number,
    timeout: number,
    message: string,
    done: (value: string | null) => void,
  ) => {
    const deadline = window.performance.now() + timeout;

    const visibleRowCount = (): number => {
      const rowWindow = Array.from(document.querySelectorAll(windowSelector)).find((candidate) =>
        candidate instanceof HTMLElement && candidate.offsetParent !== null
      );
      return rowWindow instanceof HTMLElement ? rowWindow.querySelectorAll(cssSelector).length : 0;
    };

    const tick = (): void => {
      if (visibleRowCount() >= minCount) {
        done(null);
        return;
      }
      if (window.performance.now() >= deadline) {
        done(message);
        return;
      }
      window.requestAnimationFrame(tick);
    };

    tick();
  }, rowWindowSelector, rowSelector, minimum, timeoutMs, timeoutMessage);

  if (typeof result === "string" && result !== "") {
    throw new Error(result);
  }
}

export async function openNativeRecipe(recipe: Recipe, batchId: string): Promise<void> {
  const result = await browser.executeAsync((recipeValue: Recipe, batch: string, done: (result: string | null) => void) => {
    const api = window.__GTL_VIEWER_TEST__;
    if (api === undefined) {
      done("window.__GTL_VIEWER_TEST__.openNativeRecipe is unavailable");
      return;
    }

    api.openNativeRecipe(recipeValue, batch)
      .then(() => done(null))
      .catch((error: unknown) => done(error instanceof Error ? error.message : String(error)));
  }, recipe, batchId);

  if (typeof result === "string" && result !== "") {
    throw new Error(result);
  }
}

export async function activateNativeRepo(repoName: string): Promise<void> {
  const result = await browser.executeAsync((name: string, done: (result: string | null) => void) => {
    const api = window.__GTL_VIEWER_TEST__;
    if (api === undefined) {
      done("window.__GTL_VIEWER_TEST__.activateNativeRepo is unavailable");
      return;
    }

    api.activateNativeRepo(name)
      .then((activated) => done(activated ? null : `Could not find native tab for ${name}`))
      .catch((error: unknown) => done(error instanceof Error ? error.message : String(error)));
  }, repoName);

  if (typeof result === "string" && result !== "") {
    throw new Error(result);
  }
}

/** Drives the real `--all`/`subrepos` batch-open path (freshness marking + toast) for `batch`. */
export async function openRecipeBatch(batch: OpenRecipes): Promise<void> {
  const result = await browser.executeAsync((batchValue: OpenRecipes, done: (result: string | null) => void) => {
    const api = window.__GTL_VIEWER_TEST__;
    if (api === undefined) {
      done("window.__GTL_VIEWER_TEST__.openRecipeBatch is unavailable");
      return;
    }

    api.openRecipeBatch(batchValue)
      .then(() => done(null))
      .catch((error: unknown) => done(error instanceof Error ? error.message : String(error)));
  }, batch);

  if (typeof result === "string" && result !== "") {
    throw new Error(result);
  }
}

/** Focuses a restored live tab by its saved source path (see `activateLiveTabBySource` in
 * `App.svelte` for why `activateNativeRepo` cannot reach an uncomputed live tab). */
export async function activateLiveTabBySource(sourceValue: string): Promise<void> {
  const result = await browser.execute((value: string) => {
    const api = window.__GTL_VIEWER_TEST__;
    if (api === undefined) return "window.__GTL_VIEWER_TEST__.activateLiveTabBySource is unavailable";
    return api.activateLiveTabBySource(value) ? null : `Could not find a live tab for source ${value}`;
  }, sourceValue);

  if (typeof result === "string" && result !== "") {
    throw new Error(result);
  }
}

export async function openNativeRecipeMeasured(recipe: Recipe, batchId: string): Promise<OpenMetrics> {
  const outcome = await browser.executeAsync((
    recipeValue: Recipe,
    batch: string,
    diffRootSelector: string,
    done: (result: OpenOutcome) => void,
  ) => {
    const api = window.__GTL_VIEWER_TEST__;
    if (api === undefined) {
      done({ ok: false, error: "window.__GTL_VIEWER_TEST__.openNativeRecipe is unavailable" });
      return;
    }

    const metrics: {
      startedAt: number;
      shellVisibleAt: number | null;
      resolvedAt: number | null;
      batchId: string;
    } = {
      startedAt: window.performance.now(),
      shellVisibleAt: null,
      resolvedAt: null,
      batchId: batch,
    };

    const markShellVisible = (): boolean => {
      const node = document.querySelector(diffRootSelector);
      if (node instanceof HTMLElement && node.offsetParent !== null && metrics.shellVisibleAt === null) {
        metrics.shellVisibleAt = window.performance.now();
        return true;
      }
      return false;
    };

    let observer: MutationObserver | null = null;
    if (!markShellVisible()) {
      observer = new MutationObserver(() => {
        if (markShellVisible()) {
          observer?.disconnect();
        }
      });
      observer.observe(document.body, { attributes: true, childList: true, subtree: true });
    }

    api.openNativeRecipe(recipeValue, batch)
      .then(() => {
        observer?.disconnect();
        if (metrics.shellVisibleAt === null) {
          metrics.shellVisibleAt = window.performance.now();
        }
        metrics.resolvedAt = window.performance.now();
        done({
          ok: true,
          metrics: {
            startedAt: metrics.startedAt,
            shellVisibleAt: metrics.shellVisibleAt,
            resolvedAt: metrics.resolvedAt,
            batchId: metrics.batchId,
          },
        });
      })
      .catch((error: unknown) => {
        observer?.disconnect();
        done({ ok: false, error: error instanceof Error ? error.message : String(error) });
      });
  }, recipe, batchId, selectors.diffRoot);

  if (!isOpenOutcome(outcome)) {
    throw new Error("Malformed measured open result from the native perf harness");
  }
  if (!outcome.ok) {
    throw new Error(outcome.error);
  }
  return outcome.metrics;
}

type MeasureOutcome =
  | { readonly ok: true; readonly ms: number }
  | { readonly ok: false; readonly error: string };

export type ScrollPerfMetrics = {
  readonly effectiveFps: number;
  readonly longestFrameMs: number;
  readonly frames: number;
  readonly durationMs: number;
  readonly travelledPx: number;
};

type ScrollPerfOutcome =
  | { readonly ok: true; readonly metrics: ScrollPerfMetrics }
  | { readonly ok: false; readonly error: string };

function isMeasureOutcome(value: unknown): value is MeasureOutcome {
  if (!isObjectRecord(value) || typeof value.ok !== "boolean") return false;
  if (value.ok) return typeof value.ms === "number";
  return typeof value.error === "string";
}

function isScrollPerfOutcome(value: unknown): value is ScrollPerfOutcome {
  if (!isObjectRecord(value) || typeof value.ok !== "boolean") return false;
  if (!value.ok) return typeof value.error === "string";
  if (!isObjectRecord(value.metrics)) return false;
  return (
    typeof value.metrics.effectiveFps === "number" &&
    typeof value.metrics.longestFrameMs === "number" &&
    typeof value.metrics.frames === "number" &&
    typeof value.metrics.durationMs === "number" &&
    typeof value.metrics.travelledPx === "number"
  );
}

export async function rowWindowSignature(
  rowWindowSelector = selectors.rowWindow,
  rowSelector = selectors.row,
): Promise<string> {
  return browser.execute((windowSelector: string, cssSelector: string) => {
    const rowWindow = Array.from(document.querySelectorAll(windowSelector)).find((candidate) =>
      candidate instanceof HTMLElement && candidate.offsetParent !== null
    );
    const rows =
      rowWindow instanceof HTMLElement
        ? Array.from(rowWindow.querySelectorAll(cssSelector)).slice(0, 3)
        : [];
    const rowSignature = rows
      .map((row) => {
        const text = row.textContent?.replace(/\s+/g, " ").trim().slice(0, 160) ?? "";
        return `${row.childElementCount}:${text}`;
      })
      .join("||");

    if (!(rowWindow instanceof HTMLElement)) {
      return `missing:${rows.length}:${rowSignature}`;
    }

    return [
      rows.length,
      rowWindow.scrollHeight,
      rowWindow.clientHeight,
      rowSignature,
    ].join("::");
  }, rowWindowSelector, rowSelector);
}

export async function measureClickUntilRowWindowChanges(
  actionSelector: string,
  rowWindowSelector: string,
  rowSelector: string,
  previousSignature: string,
  timeoutMs: number,
  timeoutMessage: string,
): Promise<number> {
  const outcome = await browser.executeAsync((
    buttonSelector: string,
    windowSelector: string,
    targetRowSelector: string,
    oldSignature: string,
    timeout: number,
    message: string,
    done: (result: MeasureOutcome) => void,
  ) => {
    const button = document.querySelector(buttonSelector);
    if (!(button instanceof HTMLElement)) {
      done({ ok: false, error: `Missing button ${buttonSelector}` });
      return;
    }

    const signature = (): string => {
      const rowWindow = Array.from(document.querySelectorAll(windowSelector)).find((candidate) =>
        candidate instanceof HTMLElement && candidate.offsetParent !== null
      );
      const rows =
        rowWindow instanceof HTMLElement
          ? Array.from(rowWindow.querySelectorAll(targetRowSelector)).slice(0, 3)
          : [];
      const rowSignature = rows
        .map((row) => {
          const text = row.textContent?.replace(/\s+/g, " ").trim().slice(0, 160) ?? "";
          return `${row.childElementCount}:${text}`;
        })
        .join("||");

      if (!(rowWindow instanceof HTMLElement)) {
        return `missing:${rows.length}:${rowSignature}`;
      }

      return [
        rows.length,
        rowWindow.scrollHeight,
        rowWindow.clientHeight,
        rowSignature,
      ].join("::");
    };

    const startedAt = window.performance.now();
    const deadline = startedAt + timeout;
    button.click();

    const tick = (): void => {
      const rowWindow = Array.from(document.querySelectorAll(windowSelector)).find((candidate) =>
        candidate instanceof HTMLElement && candidate.offsetParent !== null
      );
      const visibleRowCount =
        rowWindow instanceof HTMLElement ? rowWindow.querySelectorAll(targetRowSelector).length : 0;
      if (visibleRowCount > 0 && signature() !== oldSignature) {
        done({ ok: true, ms: window.performance.now() - startedAt });
        return;
      }
      if (window.performance.now() >= deadline) {
        done({ ok: false, error: message });
        return;
      }
      window.requestAnimationFrame(tick);
    };

    tick();
  }, actionSelector, rowWindowSelector, rowSelector, previousSignature, timeoutMs, timeoutMessage);

  if (!isMeasureOutcome(outcome)) {
    throw new Error("Malformed in-browser measurement result");
  }
  if (!outcome.ok) {
    throw new Error(outcome.error);
  }
  return outcome.ms;
}

export async function measureClickFirstInactiveTabUntilTextChanges(
  tabsSelector: string,
  targetSelector: string,
  previousText: string,
  timeoutMs: number,
  timeoutMessage: string,
): Promise<number> {
  const outcome = await browser.executeAsync((
    tabSelector: string,
    textSelector: string,
    oldText: string,
    timeout: number,
    message: string,
    done: (result: MeasureOutcome) => void,
  ) => {
    const tab = Array.from(document.querySelectorAll(tabSelector)).find((candidate) =>
      candidate instanceof HTMLElement && candidate.getAttribute("aria-selected") === "false"
    );
    if (!(tab instanceof HTMLElement)) {
      done({ ok: false, error: "Expected at least one inactive native tab to switch to" });
      return;
    }

    const currentText = (): string => document.querySelector(textSelector)?.textContent?.trim() ?? "";

    const startedAt = window.performance.now();
    const deadline = startedAt + timeout;
    tab.click();

    const tick = (): void => {
      if (currentText() !== oldText) {
        done({ ok: true, ms: window.performance.now() - startedAt });
        return;
      }
      if (window.performance.now() >= deadline) {
        done({ ok: false, error: message });
        return;
      }
      window.requestAnimationFrame(tick);
    };

    tick();
  }, tabsSelector, targetSelector, previousText, timeoutMs, timeoutMessage);

  if (!isMeasureOutcome(outcome)) {
    throw new Error("Malformed in-browser measurement result");
  }
  if (!outcome.ok) {
    throw new Error(outcome.error);
  }
  return outcome.ms;
}

export async function measureSustainedScroll(
  rowWindowSelector: string,
  durationMs: number,
): Promise<ScrollPerfMetrics> {
  const outcome = await browser.executeAsync((
    windowSelector: string,
    duration: number,
    done: (result: ScrollPerfOutcome) => void,
  ) => {
    const rowWindow = Array.from(document.querySelectorAll(windowSelector)).find((candidate) =>
      candidate instanceof HTMLElement && candidate.offsetParent !== null
    );
    if (!(rowWindow instanceof HTMLElement)) {
      done({ ok: false, error: `Missing row window ${windowSelector}` });
      return;
    }

    const maxScrollTop = Math.max(0, rowWindow.scrollHeight - rowWindow.clientHeight);
    if (maxScrollTop <= 0) {
      done({ ok: false, error: "row window is not scrollable" });
      return;
    }

    const travelPx = Math.min(maxScrollTop, Math.max(rowWindow.clientHeight * 24, 24_000));
    const startScrollTop = Math.min(rowWindow.scrollTop, Math.max(0, maxScrollTop - travelPx));
    const endScrollTop = Math.min(maxScrollTop, startScrollTop + travelPx);

    rowWindow.scrollTop = startScrollTop;

    let firstFrameAt: number | null = null;
    let lastAt: number | null = null;
    let frames = 0;
    let longestFrameMs = 0;

    const tick = (now: number): void => {
      if (firstFrameAt === null) {
        firstFrameAt = now;
      }
      if (lastAt !== null) {
        const delta = now - lastAt;
        longestFrameMs = Math.max(longestFrameMs, delta);
      }

      const elapsed = now - firstFrameAt;
      const progress = Math.min(1, elapsed / duration);
      rowWindow.scrollTop = startScrollTop + (endScrollTop - startScrollTop) * progress;
      frames += 1;
      lastAt = now;

      if (progress >= 1) {
        const durationMsActual = Math.max(now - firstFrameAt, 1);
        done({
          ok: true,
          metrics: {
            effectiveFps: (frames * 1000) / durationMsActual,
            longestFrameMs,
            frames,
            durationMs: durationMsActual,
            travelledPx: Math.abs(rowWindow.scrollTop - startScrollTop),
          },
        });
        return;
      }

      window.requestAnimationFrame(tick);
    };

    window.requestAnimationFrame(tick);
  }, rowWindowSelector, durationMs);

  if (!isScrollPerfOutcome(outcome)) {
    throw new Error("Malformed sustained-scroll perf result");
  }
  if (!outcome.ok) {
    throw new Error(outcome.error);
  }
  return outcome.metrics;
}
