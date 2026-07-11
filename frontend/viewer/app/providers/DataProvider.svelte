<script lang="ts">
  import { QueryClient, QueryClientProvider } from "@tanstack/svelte-query";
  import type { Snippet } from "svelte";
  import { createApiClient, createTauriTransport, provideApi } from "@/shared/api";

  const { children }: { children: Snippet } = $props();
  const api = createApiClient(createTauriTransport());
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false },
      mutations: { retry: false },
    },
  });
  provideApi(api);
</script>

<QueryClientProvider client={queryClient}>
  {@render children()}
</QueryClientProvider>
