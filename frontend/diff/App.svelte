<script lang="ts">
  import { onMount } from "svelte";
  import { enhanceControls } from "./controls";
  import { enhanceLayout } from "./enhance-layout";

  type Props = { readonly root: HTMLElement };
  let { root }: Props = $props();

  onMount(() => {
    enhanceControls(root);
    enhanceLayout(root);
  });
</script>
