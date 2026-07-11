import { createMutation, createQuery, queryOptions, useQueryClient, type QueryClient } from "@tanstack/svelte-query";
import {
  closeTabResultSchema,
  openedTabSchema,
  openRecipeBatchesSchema,
  tabMetaSchema,
  useApi,
  type ApiClient,
  type OpenedTab,
  type Recipe,
  type TabMeta,
} from "@/shared/api";

export const pendingRecipeBatchesKey = ["viewer", "pending-recipe-batches"] as const;

export function tabPrefix(tabId: number) {
  return ["viewer", "tab", tabId] as const;
}

export function tabMetaKey(tabId: number) {
  return [...tabPrefix(tabId), "meta"] as const;
}

export const openRecipeMutationKey = ["viewer", "tab", "open"] as const;

export function refreshTabMutationKey(tabId: number) {
  return [...tabPrefix(tabId), "refresh"] as const;
}

export function closeTabMutationKey(tabId: number) {
  return [...tabPrefix(tabId), "close"] as const;
}

export function tabMetaOptions(api: ApiClient, tabId: number) {
  return queryOptions({
    queryKey: tabMetaKey(tabId),
    queryFn: ({ signal }) => api.query("tab_meta", { tabId }, tabMetaSchema, signal),
    staleTime: Infinity,
  });
}

export function createPendingRecipeBatchesQuery() {
  const api = useApi();
  return createQuery({
    queryKey: pendingRecipeBatchesKey,
    queryFn: ({ signal }) => api.query("drain_pending_recipes", {}, openRecipeBatchesSchema, signal),
    enabled: false,
    staleTime: Infinity,
  });
}

export function createTabMetaQuery(tabId: number) {
  return createQuery(tabMetaOptions(useApi(), tabId));
}

export type OpenRecipeInput = {
  readonly recipe: Recipe;
  readonly batchId: string;
};

type MutationDependencies = {
  readonly api: ApiClient;
  readonly queryClient: QueryClient;
};

export function createOpenRecipeMutation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return createMutation<OpenedTab, Error, OpenRecipeInput>({
    mutationKey: openRecipeMutationKey,
    mutationFn: ({ recipe, batchId }) => api.mutate("open_recipe", { recipe, batchId }, openedTabSchema),
    onSuccess: (opened) => {
      queryClient.setQueryData(tabMetaKey(opened.tab_id), opened.meta);
    },
  });
}

export function createRefreshTabMutation(tabId: number, dependencies?: MutationDependencies) {
  const api = dependencies?.api ?? useApi();
  const queryClient = dependencies?.queryClient ?? useQueryClient();
  return createMutation<TabMeta, Error, void>(
    {
      mutationKey: refreshTabMutationKey(tabId),
      mutationFn: () => api.mutate("refresh_tab", { tabId }, tabMetaSchema),
    },
    queryClient,
  );
}

export function createCloseTabMutation(tabId: number, dependencies?: MutationDependencies) {
  const api = dependencies?.api ?? useApi();
  const queryClient = dependencies?.queryClient ?? useQueryClient();
  return createMutation<boolean, Error, void>(
    {
      mutationKey: closeTabMutationKey(tabId),
      mutationFn: () => api.mutate("close_tab", { tabId }, closeTabResultSchema),
    },
    queryClient,
  );
}

export async function removeTabQueries(queryClient: QueryClient, tabId: number): Promise<void> {
  const queryKey = tabPrefix(tabId);
  await queryClient.cancelQueries({ queryKey });
  queryClient.removeQueries({ queryKey });
}
