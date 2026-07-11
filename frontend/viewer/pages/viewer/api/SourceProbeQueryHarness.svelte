<script lang="ts">
  import { untrack } from "svelte";
  import { createSourceProbeQuery, type SourceIdentity } from "./live-views.svelte";

  const { source, enabled }: { readonly source: SourceIdentity; readonly enabled: boolean } = $props();
  const probe = createSourceProbeQuery(
    untrack(() => source),
    untrack(() => enabled),
  );
</script>

<p data-testid="source-probe-status">{$probe.status}:{$probe.fetchStatus}</p>
<p data-testid="source-probe-outcome">{$probe.data?.outcome ?? "none"}</p>
