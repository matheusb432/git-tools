<script lang="ts">
  import { createCloseTabMutation, createOpenRecipeMutation, createRefreshTabMutation } from "./recipe-tabs.svelte";
  import { recipe } from "@/shared/test";

  const open = createOpenRecipeMutation();
  const refresh = createRefreshTabMutation(7);
  const close = createCloseTabMutation(7);
</script>

<p data-testid="open-status">{$open.status}</p>
<p data-testid="refresh-status">{$refresh.status}</p>
<p data-testid="close-status">{$close.status}</p>
<button onclick={() => $open.mutate({ recipe, batchId: "batch-1" })}>Open recipe</button>
<button onclick={() => $refresh.mutate()}>Refresh tab</button>
<button onclick={() => $close.mutate()}>Close tab</button>
