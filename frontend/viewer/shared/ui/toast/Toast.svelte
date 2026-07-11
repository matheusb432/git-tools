<script lang="ts">
  import { fly } from "svelte/transition";
  import type { ToastItem } from "./toast";
  import { TEST_IDS } from "@/shared/testids";

  const { toasts, ondismiss }: { toasts: readonly ToastItem[]; ondismiss: (id: number) => void } = $props();
</script>

{#if toasts.length > 0}
  <div
    class="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex flex-col items-center gap-2"
    data-testid={TEST_IDS.toast.root}
  >
    {#each toasts as toast (toast.id)}
      <div
        transition:fly={{ y: 8, duration: 150 }}
        data-testid={TEST_IDS.toast.item}
        role="status"
        class="bg-card text-card-foreground pointer-events-auto flex items-center gap-3 rounded-lg border border-border px-3.5 py-2 text-sm shadow-lg"
      >
        <span>{toast.message}</span>
        <button
          type="button"
          class="text-foreground-muted hover:text-foreground"
          aria-label="Dismiss"
          onclick={() => ondismiss(toast.id)}
        >
          &times;
        </button>
      </div>
    {/each}
  </div>
{/if}
