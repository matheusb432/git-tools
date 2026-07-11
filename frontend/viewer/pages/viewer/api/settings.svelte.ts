import { createMutation, createQuery, useQueryClient } from "@tanstack/svelte-query";
import { settingValueSchema, unitSchema, useApi } from "@/shared/api";
import type { DiffLayout } from "../model/file-panels";

export type { DiffLayout } from "../model/file-panels";
export type DiffDensity = "compact" | "full";
export type ViewerSettings = {
  readonly layout: DiffLayout;
  readonly density: DiffDensity;
};
type ViewerSettingKey = keyof ViewerSettings;
type ViewerSettingsSnapshot<K extends ViewerSettingKey> = { readonly previous: ViewerSettings[K] };

export const viewerSettingsKey = ["viewer", "settings"] as const;
export const viewerSettingsDefaults: ViewerSettings = { layout: "unified", density: "compact" };

export function layoutFromSetting(value: string | null): DiffLayout {
  return value === "split" ? "split" : "unified";
}

export function densityFromSetting(value: string | null): DiffDensity {
  return value === "full" ? "full" : "compact";
}

export function settingFromLayout(layout: DiffLayout): string {
  return layout;
}

export function settingFromDensity(density: DiffDensity): string {
  return density;
}

export function viewerSettingsFromQuery(isError: boolean, data: ViewerSettings | undefined): ViewerSettings {
  return isError ? viewerSettingsDefaults : (data ?? viewerSettingsDefaults);
}

export function createViewerSettings() {
  const api = useApi();
  return createQuery({
    queryKey: viewerSettingsKey,
    queryFn: async ({ signal }) => {
      const [layout, density] = await Promise.all([
        api.query("get_setting", { key: "diff.layout" }, settingValueSchema, signal),
        api.query("get_setting", { key: "diff.full" }, settingValueSchema, signal),
      ]);
      return {
        layout: layoutFromSetting(layout),
        density: densityFromSetting(density),
      } satisfies ViewerSettings;
    },
    staleTime: Infinity,
  });
}

function withSetting<K extends ViewerSettingKey>(
  settings: ViewerSettings,
  key: K,
  value: ViewerSettings[K],
): ViewerSettings {
  const next: { -readonly [P in keyof ViewerSettings]: ViewerSettings[P] } = { ...settings };
  next[key] = value;
  return next;
}

const settingStorageKeys = {
  layout: "diff.layout",
  density: "diff.full",
} as const satisfies Record<ViewerSettingKey, string>;

export function createSetViewerSetting<K extends ViewerSettingKey>(key: K) {
  const api = useApi();
  const queryClient = useQueryClient();

  return createMutation<void, Error, ViewerSettings[K], ViewerSettingsSnapshot<K>>({
    mutationFn: async (value) => api.mutate("set_setting", { key: settingStorageKeys[key], value }, unitSchema),
    onMutate: async (value) => {
      await queryClient.cancelQueries({ queryKey: viewerSettingsKey, exact: true });
      const previous = queryClient.getQueryData<ViewerSettings>(viewerSettingsKey) ?? viewerSettingsDefaults;
      queryClient.setQueryData<ViewerSettings>(viewerSettingsKey, (current = viewerSettingsDefaults) =>
        withSetting(current, key, value),
      );
      return { previous: previous[key] };
    },
    onError: (_error, _value, snapshot) => {
      if (snapshot === undefined) return;
      queryClient.setQueryData<ViewerSettings>(viewerSettingsKey, (current = viewerSettingsDefaults) =>
        withSetting(current, key, snapshot.previous),
      );
    },
    onSuccess: (_data, value) => {
      queryClient.setQueryData<ViewerSettings>(viewerSettingsKey, (current = viewerSettingsDefaults) =>
        withSetting(current, key, value),
      );
    },
    scope: { id: `viewer-settings:${key}` },
  });
}
