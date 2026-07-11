<script lang="ts">
  import { useQueryClient } from "@tanstack/svelte-query";
  import { z } from "zod";
  import { useApi } from "@/shared/api";

  const response = useApi().query("provider_probe", {}, z.object({ value: z.number() }));
  const defaultOptions = useQueryClient().getDefaultOptions();
</script>

{#await response}
  <output data-testid="data-provider-probe">pending</output>
{:then result}
  <output
    data-testid="data-provider-probe"
    data-query-retry={String(defaultOptions.queries?.retry)}
    data-refetch-on-window-focus={String(defaultOptions.queries?.refetchOnWindowFocus)}
    data-mutation-retry={String(defaultOptions.mutations?.retry)}
  >
    {result.value}
  </output>
{/await}
