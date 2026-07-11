import { createMutation, createQuery, useQueryClient } from "@tanstack/svelte-query";
import { settingValueSchema, unitSchema, useApi } from "@/shared/api";
import type { DiffLayout } from "../model/file-panels";

export type { DiffLayout } from "../model/file-panels";
export type DiffDensity = "compact" | "full";
export type ViewerSettings = {
  readonly layout: DiffLayout;
  readonly density: DiffDensity;
};
export type SetViewerSetting =
  | { readonly key: "layout"; readonly value: DiffLayout }
  | { readonly key: "density"; readonly value: DiffDensity };
type ViewerSettingsSnapshot = {
  readonly previous: ViewerSettings | undefined;
};

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

export function createSetViewerSetting() {
  const api = useApi();
  const queryClient = useQueryClient();

  return createMutation<void, Error, SetViewerSetting, ViewerSettingsSnapshot>({
    mutationFn: async (setting) => {
      if (setting.key === "layout") {
        return api.mutate("set_setting", { key: "diff.layout", value: settingFromLayout(setting.value) }, unitSchema);
      }
      return api.mutate("set_setting", { key: "diff.full", value: settingFromDensity(setting.value) }, unitSchema);
    },
    onMutate: async (setting) => {
      await queryClient.cancelQueries({ queryKey: viewerSettingsKey, exact: true });
      const previous = queryClient.getQueryData<ViewerSettings>(viewerSettingsKey);
      queryClient.setQueryData<ViewerSettings>(viewerSettingsKey, (current = viewerSettingsDefaults) =>
        setting.key === "layout" ? { ...current, layout: setting.value } : { ...current, density: setting.value },
      );
      return { previous };
    },
    onError: (_error, _setting, snapshot) => {
      if (snapshot?.previous === undefined) {
        queryClient.removeQueries({ queryKey: viewerSettingsKey, exact: true });
        return;
      }
      queryClient.setQueryData(viewerSettingsKey, snapshot.previous);
    },
    scope: { id: "viewer-settings" },
  });
}
