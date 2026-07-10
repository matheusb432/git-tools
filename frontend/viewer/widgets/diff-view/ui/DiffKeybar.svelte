<script lang="ts">
  import type { TabMeta } from "@/shared/api";
  import { TEST_IDS } from "@/shared/testids";

  type Props = {
    readonly meta: TabMeta;
  };

  let { meta }: Props = $props();

  const command = $derived(`${meta.cmd_lead}${meta.cmd_range}${meta.cmd_trail}`);
</script>

<footer
  data-testid={TEST_IDS.diffView.keybar}
  class="keybar flex min-w-0 items-center gap-4 overflow-hidden border-t border-border bg-surface px-5 py-2 font-mono text-[11.5px] text-foreground-dim"
>
  <span class="min-w-0 flex-1 truncate text-foreground-muted" title={command}>
    <span class="text-foreground-dim">{meta.cmd_lead}</span><span>{meta.cmd_range}</span><span
      class="text-foreground-dim">{meta.cmd_trail}</span
    >
  </span>
  <span class="shrink-0">j/k files</span>
  <span class="shrink-0">u unified</span>
  <span class="shrink-0">s split</span>
  <span class="shrink-0">f full</span>
  <span class="shrink-0">c copy</span>
</footer>
