<script lang="ts">
  import { createLiveViewsQuery } from "./live-views.svelte";

  const liveViews = createLiveViewsQuery();
</script>

<p data-testid="live-views-status">{$liveViews.status}</p>
<p data-testid="live-views-error">{$liveViews.error?.message ?? "none"}</p>
<p data-testid="live-views-count">{$liveViews.data?.length ?? 0}</p>
