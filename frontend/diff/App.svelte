<script lang="ts">
  import { onMount } from "svelte";
  import { enhanceControls } from "./controls";
  import { enhanceLayout } from "./enhance-layout";

  type Props = { readonly root: HTMLElement };
  let { root }: Props = $props();

  onMount(() => {
    const cleanups = [enhanceControls(root), enhanceLayout(root)];
    return () => {
      cleanups.reverse().forEach((cleanup) => cleanup());
    };
  });
</script>
