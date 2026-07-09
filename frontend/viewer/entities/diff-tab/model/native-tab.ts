import type { OpenedTab, Recipe, TabMeta } from "@/shared/api";
import type { LiveTab } from "./live-tab";

export type NativeTabLifecycle =
  | { readonly state: "opening" }
  | { readonly state: "ready" }
  | { readonly state: "refreshing" }
  | { readonly state: "error"; readonly message: string };

export type NativeTabFreshness = { readonly state: "fresh" } | { readonly state: "stale"; readonly reason: string };

export type NativeTab = {
  readonly kind: "native";
  readonly localId: string;
  readonly tabId: number | null;
  readonly recipe: Recipe;
  readonly batchId: string;
  readonly meta: TabMeta | null;
  readonly lifecycle: NativeTabLifecycle;
  readonly freshness: NativeTabFreshness;
  /** True while this tab belongs to the latest opened batch and hasn't been focused yet. */
  readonly isNew?: boolean;
};

export type ViewerTab = NativeTab | LiveTab;
export type NativeTabRetryPlan =
  | { readonly kind: "open"; readonly tab: NativeTab }
  | { readonly kind: "refresh"; readonly tab: NativeTab; readonly tabId: number };

export function nativeOpeningTab(localId: string, recipe: Recipe, batchId: string): NativeTab {
  return {
    kind: "native",
    localId,
    tabId: null,
    recipe,
    batchId,
    meta: null,
    lifecycle: { state: "opening" },
    freshness: { state: "fresh" },
  };
}

export function nativeReadyTab(tab: NativeTab, opened: OpenedTab): NativeTab {
  return {
    ...tab,
    tabId: opened.tab_id,
    batchId: opened.meta.batch_id,
    meta: opened.meta,
    lifecycle: { state: "ready" },
    freshness: { state: "fresh" },
  };
}

export function nativeRefreshingTab(tab: NativeTab): NativeTab {
  return {
    ...tab,
    lifecycle: { state: "refreshing" },
    freshness: { state: "stale", reason: "refreshing" },
  };
}

export function nativeRefreshSucceeded(tab: NativeTab, meta: TabMeta): NativeTab {
  return {
    ...tab,
    tabId: meta.tab_id,
    batchId: meta.batch_id,
    meta,
    lifecycle: { state: "ready" },
    freshness: { state: "fresh" },
  };
}

export function nativeTabError(tab: NativeTab, message: string): NativeTab {
  return {
    ...tab,
    lifecycle: { state: "error", message },
  };
}

/** Flags every tab from `batchId` as new; clears the cue on every other tab. */
export function markBatchFresh(tabs: readonly ViewerTab[], batchId: string): ViewerTab[] {
  return tabs.map((tab) => ({ ...tab, isNew: tab.kind === "native" && tab.batchId === batchId }));
}

/** Clears the "new" cue on the tab at `index` (called when that tab is focused). */
export function clearTabFreshness(tabs: readonly ViewerTab[], index: number): ViewerTab[] {
  return tabs.map((tab, i) => (i === index ? { ...tab, isNew: false } : tab));
}

export function beginNativeTabRetry(tab: NativeTab): NativeTabRetryPlan | null {
  if (tab.lifecycle.state === "opening" || tab.lifecycle.state === "refreshing") return null;

  if (tab.tabId === null) {
    return {
      kind: "open",
      tab: {
        ...tab,
        lifecycle: { state: "opening" },
      },
    };
  }

  return {
    kind: "refresh",
    tab: nativeRefreshingTab(tab),
    tabId: tab.tabId,
  };
}
