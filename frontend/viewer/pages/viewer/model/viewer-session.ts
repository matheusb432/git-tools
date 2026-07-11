import { createStore } from "@xstate/store-svelte";
import type { Recipe, RecipeSource, TabMeta } from "@/shared/api";
import type { ToastItem } from "@/shared/ui/toast";
import type { BrokenSource } from "./live-tab";

export type LiveShell = {
  readonly source: RecipeSource;
  readonly displayName: string;
  readonly needsActivation: boolean;
  readonly brokenSource: BrokenSource | null;
};

export type TabShell = {
  readonly localId: string;
  readonly recipe: Recipe;
  readonly batchId: string;
  readonly tabId: number | null;
  readonly isNew: boolean;
  readonly failure: string | null;
  readonly live: LiveShell | null;
};

export type ViewerSessionContext = {
  readonly tabs: readonly TabShell[];
  readonly activeLocalId: string | null;
  readonly toasts: readonly ToastItem[];
  readonly shellNotice: string | null;
};

export type ViewerSessionEventPayloads = {
  readonly "tab.openRequested": { readonly tab: TabShell };
  readonly "tab.opened": { readonly localId: string; readonly tabId: number; readonly batchId: string };
  readonly "tab.failed": { readonly localId: string; readonly message: string };
  readonly "tab.focused": { readonly localId: string };
  readonly "tab.closed": { readonly localId: string };
  readonly "tabs.closedOthers": { readonly keepLocalId: string };
  readonly "tabs.closedAll": null;
  readonly "batch.markedFresh": { readonly batchId: string };
  readonly "liveViews.restored": { readonly tabs: readonly TabShell[] };
  readonly "liveView.activated": { readonly localId: string };
  readonly "liveView.broken": { readonly localId: string; readonly brokenSource: BrokenSource };
  readonly "toast.added": { readonly toast: ToastItem };
  readonly "toast.dismissed": { readonly id: number };
  readonly "shellNotice.changed": { readonly message: string | null };
};

function patchContext(context: ViewerSessionContext, patch: Partial<ViewerSessionContext>): ViewerSessionContext {
  return { ...context, ...patch };
}

function updateTab(
  context: ViewerSessionContext,
  localId: string,
  update: (tab: TabShell) => TabShell,
): ViewerSessionContext {
  let changed = false;
  const tabs = context.tabs.map((tab) => {
    if (tab.localId !== localId) return tab;
    changed = true;
    return update(tab);
  });
  return changed ? patchContext(context, { tabs }) : context;
}

function sourceKey(tab: TabShell): string | null {
  return tab.live === null ? null : `${tab.live.source.kind}\u0000${tab.live.source.value}`;
}

function nextActiveAfterClose(
  context: ViewerSessionContext,
  localId: string,
  tabs: readonly TabShell[],
): string | null {
  if (context.activeLocalId !== localId) return context.activeLocalId;
  const closedIndex = context.tabs.findIndex((tab) => tab.localId === localId);
  return tabs[closedIndex]?.localId ?? tabs.at(-1)?.localId ?? null;
}

/** Orders shells newest-first using Query-owned metadata while preserving stable order for ties. */
export function sortTabShellsByMeta(
  tabs: readonly TabShell[],
  metadata: ReadonlyMap<number, TabMeta>,
): readonly TabShell[] {
  return [...tabs]
    .map((tab, index) => ({ tab, index }))
    .sort((left, right) => {
      const leftKey =
        left.tab.tabId === null
          ? left.tab.batchId
          : (metadata.get(left.tab.tabId)?.commits[0]?.iso ?? left.tab.batchId);
      const rightKey =
        right.tab.tabId === null
          ? right.tab.batchId
          : (metadata.get(right.tab.tabId)?.commits[0]?.iso ?? right.tab.batchId);
      if (leftKey < rightKey) return 1;
      if (leftKey > rightKey) return -1;
      return left.index - right.index;
    })
    .map(({ tab }) => tab);
}

export function openedTabOutcome(
  tabs: readonly TabShell[],
  event: { readonly localId: string; readonly tabId: number },
):
  | { readonly kind: "apply" }
  | { readonly kind: "alreadyOwned"; readonly tabId: number }
  | { readonly kind: "orphaned"; readonly tabId: number } {
  if (tabs.some((tab) => tab.localId === event.localId)) return { kind: "apply" };
  return tabs.some((tab) => tab.tabId === event.tabId)
    ? { kind: "alreadyOwned", tabId: event.tabId }
    : { kind: "orphaned", tabId: event.tabId };
}

export function createViewerSessionStore() {
  return createStore<ViewerSessionContext, ViewerSessionEventPayloads>({
    context: { tabs: [], activeLocalId: null, toasts: [], shellNotice: null },
    on: {
      "tab.openRequested": (context, event) => {
        const tabs = [...context.tabs.filter((tab) => tab.localId !== event.tab.localId), event.tab];
        return patchContext(context, { tabs, activeLocalId: event.tab.localId, shellNotice: null });
      },
      "tab.opened": (context, event) => {
        if (openedTabOutcome(context.tabs, event).kind === "orphaned") return context;
        const removedDuplicateLocalIds = context.tabs
          .filter((tab) => tab.tabId === event.tabId && tab.localId !== event.localId)
          .map((tab) => tab.localId);
        const tabs = context.tabs
          .filter((tab) => tab.tabId !== event.tabId || tab.localId === event.localId)
          .map((tab) =>
            tab.localId === event.localId ? { ...tab, tabId: event.tabId, batchId: event.batchId, failure: null } : tab,
          );
        const activeWasRemoved =
          context.activeLocalId !== null && removedDuplicateLocalIds.includes(context.activeLocalId);
        return patchContext(context, {
          tabs,
          activeLocalId: activeWasRemoved ? event.localId : context.activeLocalId,
        });
      },
      "tab.failed": (context, event) =>
        updateTab(context, event.localId, (tab) => ({ ...tab, failure: event.message })),
      "tab.focused": (context, event) => {
        if (!context.tabs.some((tab) => tab.localId === event.localId)) return context;
        return patchContext(context, {
          activeLocalId: event.localId,
          tabs: context.tabs.map((tab) =>
            tab.localId === event.localId && tab.isNew ? { ...tab, isNew: false } : tab,
          ),
        });
      },
      "tab.closed": (context, event) => {
        const tabs = context.tabs.filter((tab) => tab.localId !== event.localId);
        if (tabs.length === context.tabs.length) return context;
        return patchContext(context, {
          tabs,
          activeLocalId: nextActiveAfterClose(context, event.localId, tabs),
        });
      },
      "tabs.closedOthers": (context, event) => {
        const kept = context.tabs.find((tab) => tab.localId === event.keepLocalId);
        return kept === undefined ? context : patchContext(context, { tabs: [kept], activeLocalId: event.keepLocalId });
      },
      "tabs.closedAll": (context) => patchContext(context, { tabs: [], activeLocalId: null }),
      "batch.markedFresh": (context, event) =>
        patchContext(context, {
          tabs: context.tabs.map((tab) => ({ ...tab, isNew: tab.batchId === event.batchId })),
        }),
      "liveViews.restored": (context, event) => {
        const seen = new Set(context.tabs.flatMap((tab) => sourceKey(tab) ?? []));
        const restored: TabShell[] = [];
        for (const tab of event.tabs) {
          const key = sourceKey(tab);
          if (key === null || seen.has(key)) continue;
          seen.add(key);
          restored.push(tab);
        }
        if (restored.length === 0) return context;
        return patchContext(context, {
          tabs: [...context.tabs, ...restored],
          activeLocalId: context.activeLocalId ?? restored[0]?.localId ?? null,
        });
      },
      "liveView.activated": (context, event) =>
        updateTab(context, event.localId, (tab) =>
          tab.live === null ? tab : { ...tab, live: { ...tab.live, needsActivation: false } },
        ),
      "liveView.broken": (context, event) =>
        updateTab(context, event.localId, (tab) =>
          tab.live === null
            ? { ...tab, failure: event.brokenSource.reason }
            : {
                ...tab,
                failure: event.brokenSource.reason,
                live: { ...tab.live, needsActivation: false, brokenSource: event.brokenSource },
              },
        ),
      "toast.added": (context, event) => patchContext(context, { toasts: [...context.toasts, event.toast] }),
      "toast.dismissed": (context, event) =>
        patchContext(context, { toasts: context.toasts.filter((toast) => toast.id !== event.id) }),
      "shellNotice.changed": (context, event) => patchContext(context, { shellNotice: event.message }),
    },
  });
}

export type ViewerSessionStore = ReturnType<typeof createViewerSessionStore>;
