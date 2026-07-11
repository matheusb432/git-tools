<script lang="ts">
  import { createQueries, useMutationState, useQueryClient, type QueryKey } from "@tanstack/svelte-query";
  import { useSelector } from "@xstate/store-svelte";
  import { flushSync, onMount, tick } from "svelte";
  import { derived, get } from "svelte/store";
  import {
    createCloseTabMutation,
    createOpenRecipeMutation,
    createPendingRecipeBatchesQuery,
    createRefreshTabMutation,
    removeTabQueries,
    tabMetaKey,
    tabMetaOptions,
  } from "../api/recipe-tabs.svelte";
  import { createLiveViewsQuery, createSourceProbeFetcher } from "../api/live-views.svelte";
  import { resetTabRowPages } from "../api/row-pages.svelte";
  import { createDiffReviewStore, type DiffReviewStore } from "../model/diff-review";
  import { brokenSourceFromRejection, liveTabFromDto } from "../model/live-tab";
  import {
    createViewerSessionStore,
    openedTabOutcome,
    sortTabShellsByMeta,
    type TabShell,
  } from "../model/viewer-session";
  import DiffView from "./diff-view/DiffView.svelte";
  import TabStrip from "./tab-strip/TabStrip.svelte";
  import {
    OPEN_RECIPE_EVENT,
    openRecipesWireSchema,
    useApi,
    type OpenRecipes,
    type Recipe,
    type TabMeta,
  } from "@/shared/api";
  import { batchMessage, createToast, Toast } from "@/shared/ui/toast";

  type TabLifecycle = "opening" | "ready" | "refreshing" | "error";
  type PresentedTab = TabShell & {
    readonly meta: TabMeta | null;
    readonly isRefreshing: boolean;
  };

  type ViewerTestApi = {
    openNativeRecipe: (recipe: Recipe, batchId?: string) => Promise<void>;
    activateNativeRepo: (repoName: string) => Promise<boolean>;
    announceBatch: (count: number, commandLabel: string) => void;
    openRecipeBatch: (batch: OpenRecipes) => Promise<void>;
    activateLiveTabBySource: (sourceValue: string) => boolean;
    snapshot: () => {
      readonly active: number;
      readonly activeTab: {
        readonly kind: "native";
        readonly tabId: number | null;
        readonly lifecycle: TabLifecycle;
        readonly repoName: string | null;
      } | null;
      readonly shellError: string | null;
      readonly reviewStoreCount: number;
      readonly browserErrors: readonly string[];
      readonly queryCache: {
        readonly rowPages: number;
      };
      readonly tabs: readonly {
        readonly kind: "native";
        readonly localId: string;
        readonly tabId: number | null;
        readonly batchId: string;
        readonly lifecycle: TabLifecycle;
        readonly error?: string;
        readonly repoName: string | null;
        readonly fileCount: number | null;
        readonly live: boolean;
        readonly source: { readonly kind: string; readonly value: string } | null;
      }[];
      readonly toasts: readonly { readonly id: number; readonly message: string }[];
    };
  };

  const api = useApi();
  const queryClient = useQueryClient();
  const session = createViewerSessionStore();
  const tabShells = useSelector(session, (snapshot) => snapshot.context.tabs);
  const activeLocalId = useSelector(session, (snapshot) => snapshot.context.activeLocalId);
  const toasts = useSelector(session, (snapshot) => snapshot.context.toasts);
  const shellNotice = useSelector(session, (snapshot) => snapshot.context.shellNotice);
  const pendingBatches = createPendingRecipeBatchesQuery();
  const liveViewsQuery = createLiveViewsQuery();
  const probeSource = createSourceProbeFetcher();
  const openMutation = createOpenRecipeMutation();
  const reviewStores = new Map<string, DiffReviewStore>();
  const browserErrors: string[] = [];
  let nextToastId = 0;
  let unlistenRecipe: (() => void) | undefined;
  let batchQueue = Promise.resolve();

  const metadataOptions = derived(tabShells, (tabs) =>
    tabs.flatMap((tab) => (tab.tabId === null ? [] : [tabMetaOptions(api, tab.tabId)])),
  );
  const metadataQueries = createQueries({ queries: metadataOptions });
  const refreshStates = useMutationState({
    filters: { mutationKey: ["viewer", "tab"] },
    select: (mutation) => ({ key: mutation.options.mutationKey, status: mutation.state.status }),
  });

  function isPendingRefresh(key: QueryKey | undefined, status: string, tabId: number): boolean {
    return (
      status === "pending" && key?.[0] === "viewer" && key[1] === "tab" && key[2] === tabId && key[3] === "refresh"
    );
  }

  function tabIsRefreshing(tab: TabShell): boolean {
    const tabId = tab.tabId;
    return tabId !== null && $refreshStates.some((state) => isPendingRefresh(state.key, state.status, tabId));
  }

  const metadata = $derived.by(() => {
    const result = new Map<number, TabMeta>();
    let queryIndex = 0;
    for (const tab of $tabShells) {
      if (tab.tabId === null) continue;
      const meta = $metadataQueries[queryIndex]?.data;
      queryIndex += 1;
      if (meta !== undefined) result.set(tab.tabId, meta);
    }
    return result;
  });
  const tabs = $derived<PresentedTab[]>(
    sortTabShellsByMeta($tabShells, metadata).map((tab) => ({
      ...tab,
      meta: tab.tabId === null ? null : (metadata.get(tab.tabId) ?? null),
      isRefreshing: tabIsRefreshing(tab),
    })),
  );
  const activeTab = $derived(tabs.find((tab) => tab.localId === $activeLocalId));

  function lifecycle(tab: PresentedTab): TabLifecycle {
    if (tab.failure !== null) return "error";
    if (tab.tabId === null) return "opening";
    return tab.isRefreshing ? "refreshing" : "ready";
  }

  function reviewStoreFor(localId: string): DiffReviewStore {
    const existing = reviewStores.get(localId);
    if (existing !== undefined) return existing;
    const created = createDiffReviewStore();
    reviewStores.set(localId, created);
    return created;
  }

  function dismissToast(id: number): void {
    session.trigger["toast.dismissed"]({ id });
  }

  function announceBatch(count: number, commandLabel: string): void {
    const message = batchMessage(count, commandLabel);
    if (message === null) return;
    const toast = createToast(nextToastId++, message);
    session.trigger["toast.added"]({ toast });
    setTimeout(() => dismissToast(toast.id), toast.timeoutMs);
  }

  async function closeBackendTab(tabId: number): Promise<void> {
    await removeTabQueries(queryClient, tabId);
    const closeMutation = createCloseTabMutation(tabId, { api, queryClient });
    try {
      await get(closeMutation).mutateAsync();
    } catch {
      session.trigger["shellNotice.changed"]({ message: "Failed to close one or more native tabs" });
    }
  }

  async function resolveOpenedShell(localId: string, recipe: Recipe, batchId: string): Promise<void> {
    try {
      const opened = await $openMutation.mutateAsync({ recipe, batchId });
      const outcome = openedTabOutcome(session.getSnapshot().context.tabs, { localId, tabId: opened.tab_id });
      if (outcome.kind === "orphaned") {
        await closeBackendTab(outcome.tabId);
        return;
      }
      if (outcome.kind === "alreadyOwned") return;

      const reusedLocalIds = session
        .getSnapshot()
        .context.tabs.filter((tab) => tab.tabId === opened.tab_id && tab.localId !== localId)
        .map((tab) => tab.localId);
      session.trigger["tab.opened"]({ localId, tabId: opened.tab_id, batchId: opened.meta.batch_id });
      reusedLocalIds.forEach((reusedLocalId) => reviewStores.delete(reusedLocalId));
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to open diff";
      session.trigger["tab.failed"]({ localId, message });
    }
  }

  async function openNativeRecipe(recipe: Recipe, batchId: string = crypto.randomUUID()): Promise<void> {
    const localId = crypto.randomUUID();
    session.trigger["tab.openRequested"]({
      tab: { localId, recipe, batchId, tabId: null, isNew: false, failure: null, live: null },
    });
    flushSync();
    await tick();
    await resolveOpenedShell(localId, recipe, batchId);
    flushSync();
    await tick();
  }

  function opCommandLabel(op: Recipe["op"]): string {
    switch (op.op) {
      case "diff":
        return "gtl diff";
      case "merge-diff":
        return "gtl merge-diff";
      case "squash-preview":
        return "gtl squash-preview";
    }
  }

  function batchLabel(recipes: readonly Recipe[]): string {
    const first = recipes[0];
    if (first === undefined) return "gtl diff";
    if (recipes.length === 1) return opCommandLabel(first.op);
    const sameOp = recipes.every((recipe) => recipe.op.op === first.op.op);
    return `${sameOp ? opCommandLabel(first.op) : "gtl diff"} (${recipes.length} repos)`;
  }

  async function openRecipeBatch({ batchId, recipes }: OpenRecipes): Promise<void> {
    for (const recipe of recipes) await openNativeRecipe(recipe, batchId);
    session.trigger["batch.markedFresh"]({ batchId });
    announceBatch(recipes.length, batchLabel(recipes));
  }

  async function openRecipeBatches(batches: readonly OpenRecipes[]): Promise<void> {
    for (const batch of batches) await openRecipeBatch(batch);
  }

  function enqueueRecipeBatches(batches: readonly OpenRecipes[]): void {
    batchQueue = batchQueue
      .then(() => openRecipeBatches(batches))
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Failed to open recipe batch";
        session.trigger["shellNotice.changed"]({ message });
      });
  }

  async function computeLiveTab(tab: TabShell): Promise<void> {
    if (tab.live === null || !tab.live.needsActivation) return;
    session.trigger["liveView.activated"]({ localId: tab.localId });
    try {
      const probe = await probeSource(tab.live.source);
      if (probe.outcome === "broken") {
        const brokenSource = brokenSourceFromRejection(probe.code, probe.reason) ?? {
          code: "DirNotFound" as const,
          reason: probe.reason,
        };
        session.trigger["liveView.broken"]({ localId: tab.localId, brokenSource });
        return;
      }
      await resolveOpenedShell(tab.localId, tab.recipe, tab.batchId);
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to open diff";
      session.trigger["tab.failed"]({ localId: tab.localId, message });
    }
  }

  function focusTab(localId: string): void {
    const tab = session.getSnapshot().context.tabs.find((candidate) => candidate.localId === localId);
    if (tab === undefined) return;
    session.trigger["tab.focused"]({ localId });
    if (tab.live?.needsActivation === true) void computeLiveTab(tab);
  }

  function activateLiveTabBySource(sourceValue: string): boolean {
    const tab = session.getSnapshot().context.tabs.find((candidate) => candidate.live?.source.value === sourceValue);
    if (tab === undefined) return false;
    focusTab(tab.localId);
    return true;
  }

  async function activateNativeRepo(repoName: string): Promise<boolean> {
    const tab = tabs.find((candidate) => candidate.meta?.repo_name === repoName);
    if (tab === undefined) return false;
    focusTab(tab.localId);
    flushSync();
    await tick();
    return true;
  }

  async function handleRefresh(tab: PresentedTab): Promise<void> {
    if (tab.isRefreshing) return;
    if (tab.tabId === null) {
      session.trigger["tab.openRequested"]({
        tab: {
          localId: tab.localId,
          recipe: tab.recipe,
          batchId: tab.batchId,
          tabId: null,
          isNew: tab.isNew,
          failure: null,
          live: tab.live,
        },
      });
      await resolveOpenedShell(tab.localId, tab.recipe, tab.batchId);
      return;
    }

    const tabId = tab.tabId;
    const refreshMutation = createRefreshTabMutation(tabId, { api, queryClient });
    try {
      const meta = await get(refreshMutation).mutateAsync();
      const stillOwnsTab = session
        .getSnapshot()
        .context.tabs.some((candidate) => candidate.localId === tab.localId && candidate.tabId === tabId);
      if (!stillOwnsTab) return;
      await resetTabRowPages(queryClient, tabId);
      const stillOwnsTabAfterReset = session
        .getSnapshot()
        .context.tabs.some((candidate) => candidate.localId === tab.localId && candidate.tabId === tabId);
      if (!stillOwnsTabAfterReset) return;
      queryClient.setQueryData(tabMetaKey(tabId), meta);
      session.trigger["tab.opened"]({ localId: tab.localId, tabId, batchId: meta.batch_id });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to refresh diff";
      session.trigger["tab.failed"]({ localId: tab.localId, message });
    }
  }

  async function closeShells(shells: readonly TabShell[]): Promise<void> {
    shells.forEach((tab) => reviewStores.delete(tab.localId));
    await Promise.all(shells.flatMap((tab) => (tab.tabId === null ? [] : [closeBackendTab(tab.tabId)])));
  }

  async function handleClose(localId: string): Promise<void> {
    const tab = session.getSnapshot().context.tabs.find((candidate) => candidate.localId === localId);
    if (tab === undefined) return;
    session.trigger["tab.closed"]({ localId });
    await closeShells([tab]);
  }

  async function handleCloseOthers(keepLocalId: string): Promise<void> {
    const toClose = session.getSnapshot().context.tabs.filter((tab) => tab.localId !== keepLocalId);
    session.trigger["tabs.closedOthers"]({ keepLocalId });
    await closeShells(toClose);
  }

  async function handleCloseAll(): Promise<void> {
    const toClose = session.getSnapshot().context.tabs;
    session.trigger["tabs.closedAll"]();
    await closeShells(toClose);
  }

  function currentMetadata(): Map<number, TabMeta> {
    const result = new Map<number, TabMeta>();
    for (const tab of session.getSnapshot().context.tabs) {
      if (tab.tabId === null) continue;
      const meta = queryClient.getQueryData<TabMeta>(tabMetaKey(tab.tabId));
      if (meta !== undefined) result.set(tab.tabId, meta);
    }
    return result;
  }

  function isRowPageQueryKey(key: QueryKey): boolean {
    return (
      key.length === 10 &&
      key[0] === "viewer" &&
      key[1] === "tab" &&
      typeof key[2] === "number" &&
      key[3] === "file" &&
      typeof key[4] === "number" &&
      key[5] === "rows" &&
      (key[6] === "unified" || key[6] === "split") &&
      (key[7] === "compact" || key[7] === "full") &&
      typeof key[8] === "number" &&
      typeof key[9] === "number"
    );
  }

  function snapshotViewerState(): ReturnType<ViewerTestApi["snapshot"]> {
    const context = session.getSnapshot().context;
    const cachedMetadata = currentMetadata();
    const ordered = sortTabShellsByMeta(context.tabs, cachedMetadata);
    const visible = ordered.find((tab) => tab.localId === context.activeLocalId);
    const presented = (tab: TabShell): PresentedTab => ({
      ...tab,
      meta: tab.tabId === null ? null : (cachedMetadata.get(tab.tabId) ?? null),
      isRefreshing: false,
    });
    return {
      active: Math.max(
        0,
        ordered.findIndex((tab) => tab.localId === context.activeLocalId),
      ),
      activeTab:
        visible === undefined
          ? null
          : {
              kind: "native",
              tabId: visible.tabId,
              lifecycle: lifecycle(presented(visible)),
              repoName: visible.tabId === null ? null : (cachedMetadata.get(visible.tabId)?.repo_name ?? null),
            },
      shellError: context.shellNotice,
      reviewStoreCount: reviewStores.size,
      browserErrors: [...browserErrors],
      queryCache: {
        rowPages: queryClient
          .getQueryCache()
          .getAll()
          .filter((query) => isRowPageQueryKey(query.queryKey)).length,
      },
      tabs: ordered.map((tab) => {
        const meta = tab.tabId === null ? null : (cachedMetadata.get(tab.tabId) ?? null);
        return {
          kind: "native" as const,
          localId: tab.localId,
          tabId: tab.tabId,
          batchId: tab.batchId,
          lifecycle: lifecycle({ ...tab, meta, isRefreshing: false }),
          repoName: meta?.repo_name ?? null,
          fileCount: meta?.files.length ?? null,
          live: tab.live !== null,
          source: tab.live?.source ?? null,
          ...(tab.failure === null ? {} : { error: tab.failure }),
        };
      }),
      toasts: context.toasts.map((toast) => ({ id: toast.id, message: toast.message })),
    };
  }

  function restoreLiveViews(): void {
    const priorActive = session.getSnapshot().context.activeLocalId;
    try {
      const restored = ($liveViewsQuery.data ?? []).map(liveTabFromDto);
      session.trigger["liveViews.restored"]({ tabs: restored });
      if (priorActive === null) {
        const active = session
          .getSnapshot()
          .context.tabs.find((tab) => tab.localId === session.getSnapshot().context.activeLocalId);
        if (active?.live?.needsActivation === true) void computeLiveTab(active);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to restore live views";
      session.trigger["shellNotice.changed"]({ message });
    }
  }

  onMount(() => {
    let disposed = false;
    const recordError = (value: unknown): void => {
      browserErrors.push(
        value instanceof Error ? `${value.name}: ${value.message}\n${value.stack ?? ""}` : String(value),
      );
    };
    const handleError = (event: ErrorEvent): void => recordError(event.error ?? event.message);
    const handleRejection = (event: PromiseRejectionEvent): void => recordError(event.reason);
    window.addEventListener("error", handleError);
    window.addEventListener("unhandledrejection", handleRejection);

    const testApi: ViewerTestApi = {
      openNativeRecipe,
      activateNativeRepo,
      announceBatch,
      openRecipeBatch,
      activateLiveTabBySource,
      snapshot: snapshotViewerState,
    };
    Reflect.set(window, "__GTL_VIEWER_TEST__", testApi);

    let pendingSettled = false;
    const unobservePending = pendingBatches.subscribe((result) => {
      if (pendingSettled || result.isPending) return;
      pendingSettled = true;
      if (result.isError) {
        session.trigger["shellNotice.changed"]({ message: result.error.message });
        return;
      }
      enqueueRecipeBatches(result.data);
    });
    let liveViewsSettled = false;
    const unobserveLiveViews = liveViewsQuery.subscribe((result) => {
      if (liveViewsSettled || result.isPending) return;
      liveViewsSettled = true;
      if (result.isError) {
        session.trigger["shellNotice.changed"]({ message: result.error.message });
        return;
      }
      restoreLiveViews();
    });
    api
      .listen(OPEN_RECIPE_EVENT, openRecipesWireSchema, (batch) => {
        enqueueRecipeBatches([batch]);
      })
      .then((stop) => {
        if (disposed) stop();
        else unlistenRecipe = stop;
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Failed to subscribe to recipe events";
        session.trigger["shellNotice.changed"]({ message });
      });

    return () => {
      disposed = true;
      if (Reflect.get(window, "__GTL_VIEWER_TEST__") === testApi) {
        Reflect.deleteProperty(window, "__GTL_VIEWER_TEST__");
      }
      window.removeEventListener("error", handleError);
      window.removeEventListener("unhandledrejection", handleRejection);
      unobservePending();
      unobserveLiveViews();
      unlistenRecipe?.();
      unlistenRecipe = undefined;
    };
  });
</script>

<div class="grid h-full grid-rows-[auto_1fr] bg-background text-foreground">
  <TabStrip
    {tabs}
    activeLocalId={$activeLocalId}
    onActivate={focusTab}
    onClose={handleClose}
    onCloseOthers={handleCloseOthers}
    onCloseAll={handleCloseAll}
  />

  <div class="flex min-h-0 flex-col">
    {#if $shellNotice !== null}
      <div class="border-b border-destructive/30 bg-destructive/10 px-5 py-2 text-sm text-destructive">
        {$shellNotice}
      </div>
    {/if}
    {#if activeTab !== undefined}
      {#key activeTab.localId}
        <DiffView tab={activeTab} reviewStore={reviewStoreFor(activeTab.localId)} onRefresh={handleRefresh} />
      {/key}
    {:else}
      <div class="grid min-h-0 flex-1 place-items-center text-sm text-foreground-muted">
        No diff open. Run <code class="mx-1.5 rounded bg-muted px-1.5 py-0.5 font-mono">gtl diff</code>.
      </div>
    {/if}
  </div>
</div>
<Toast toasts={$toasts} ondismiss={dismissToast} />
