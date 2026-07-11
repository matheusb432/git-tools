<script lang="ts">
  import { createSetViewerSetting, createViewerSettings, viewerSettingsFromQuery } from "./settings.svelte";

  const settings = createViewerSettings();
  const layoutMutation = createSetViewerSetting("layout");
  const densityMutation = createSetViewerSetting("density");
  const visibleSettings = $derived(viewerSettingsFromQuery($settings.isError, $settings.data));
</script>

<p data-testid="settings-status">{$settings.status}</p>
<p data-testid="settings-value">{visibleSettings.layout}:{visibleSettings.density}</p>
<p data-testid="settings-mutation-status">{$layoutMutation.status}</p>
<button onclick={() => $layoutMutation.mutate("split")}>Use split layout</button>
<button onclick={() => $densityMutation.mutate("full")}>Use full density</button>
<button onclick={() => void $settings.refetch()}>Refetch settings</button>
