import { expect, test } from "bun:test";
import {
  densityFromSetting,
  layoutFromSetting,
  settingFromDensity,
  settingFromLayout,
  viewerSettingsKey,
} from "./settings.svelte";

test("settings use one stable resource key and typed defaults", () => {
  expect(viewerSettingsKey).toEqual(["viewer", "settings"]);
  expect(layoutFromSetting(null)).toBe("unified");
  expect(layoutFromSetting("split")).toBe("split");
  expect(densityFromSetting(null)).toBe("compact");
  expect(densityFromSetting("full")).toBe("full");
  expect(settingFromLayout("split")).toBe("split");
  expect(settingFromDensity("compact")).toBe("compact");
});

test("invalid persisted settings settle to typed defaults", () => {
  expect(layoutFromSetting("stacked")).toBe("unified");
  expect(densityFromSetting("verbose")).toBe("compact");
});
