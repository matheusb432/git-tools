import { createQuery, queryOptions, useQueryClient } from "@tanstack/svelte-query";
import { liveViewsSchema, sourceProbeSchema, useApi, type ApiClient, type SourceProbe } from "@/shared/api";

export type SourceIdentity = {
  readonly kind: string;
  readonly value: string;
};

export const liveViewsKey = ["viewer", "live-views"] as const;

export function sourceProbeKey(source: SourceIdentity) {
  return ["viewer", "source-probe", source.kind, source.value] as const;
}

function sourceProbeOptions(api: ApiClient, source: SourceIdentity, enabled: boolean) {
  return queryOptions({
    queryKey: sourceProbeKey(source),
    queryFn: ({ signal }) =>
      api.query("probe_source", { sourceKind: source.kind, sourceValue: source.value }, sourceProbeSchema, signal),
    enabled,
  });
}

export function createLiveViewsQuery() {
  const api = useApi();
  return createQuery({
    queryKey: liveViewsKey,
    queryFn: ({ signal }) => api.query("list_live_views", {}, liveViewsSchema, signal),
  });
}

export function createSourceProbeQuery(source: SourceIdentity, enabled: boolean) {
  return createQuery(sourceProbeOptions(useApi(), source, enabled));
}

export function createSourceProbeFetcher(): (source: SourceIdentity) => Promise<SourceProbe> {
  const api = useApi();
  const queryClient = useQueryClient();
  return (source) => queryClient.fetchQuery(sourceProbeOptions(api, source, true));
}
