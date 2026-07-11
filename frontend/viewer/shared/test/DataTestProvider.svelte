<script lang="ts">
  import { QueryClientProvider, type QueryClient } from "@tanstack/svelte-query";
  import { untrack, type Snippet } from "svelte";
  import { provideApi, type ApiClient } from "@/shared/api";

  const {
    api,
    queryClient,
    children,
  }: { readonly api: ApiClient; readonly queryClient: QueryClient; readonly children: Snippet } = $props();

  provideApi(untrack(() => api));
</script>

<QueryClientProvider client={queryClient}>
  {@render children()}
</QueryClientProvider>
