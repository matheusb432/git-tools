<script lang="ts">
  import { createSetViewerSetting, createViewerSettings, viewerSettingsFromQuery } from "./settings.svelte";

  const settings = createViewerSettings();
  const settingMutation = createSetViewerSetting();
  const visibleSettings = $derived(viewerSettingsFromQuery($settings.isError, $settings.data));
</script>

<p data-testid="settings-status">{$settings.status}</p>
<p data-testid="settings-value">{visibleSettings.layout}:{visibleSettings.density}</p>
<p data-testid="settings-mutation-status">{$settingMutation.status}</p>
<button onclick={() => $settingMutation.mutate({ key: "layout", value: "split" })}>Use split layout</button>
<button onclick={() => void $settings.refetch()}>Refetch settings</button>
