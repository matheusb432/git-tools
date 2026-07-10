import { expect, test } from "bun:test";
import {
  FILE_PANEL_ESTIMATED_HEIGHT,
  FILE_PANEL_OVERSCAN,
  FILE_PANEL_WINDOW_TARGET,
  filePanelDomKey,
  filePanelMountedBudget,
  filePanelOverscanForViewport,
  normalizedSelectedFileIdx,
} from "./file-panel-window";

test("file panel constants keep the initial tuning target explicit", () => {
  expect(FILE_PANEL_WINDOW_TARGET).toBe(10);
  expect(FILE_PANEL_OVERSCAN).toBe(2);
  expect(filePanelMountedBudget()).toBe(14);
});

test("file panel keys are stable by file index and path", () => {
  expect(filePanelDomKey({ fileIdx: 3, path: "src/main.rs" })).toBe("3:src/main.rs");
});

test("file panel overscan keeps the target mounted budget across viewport heights", () => {
  expect(filePanelOverscanForViewport(0)).toBe(5);
  expect(filePanelOverscanForViewport(FILE_PANEL_ESTIMATED_HEIGHT * 6)).toBe(FILE_PANEL_OVERSCAN);
  expect(filePanelOverscanForViewport(FILE_PANEL_ESTIMATED_HEIGHT, 15)).toBe(7);
});

test("selected file falls back to the first visible file", () => {
  expect(normalizedSelectedFileIdx([4, 9, 12], null)).toBe(4);
  expect(normalizedSelectedFileIdx([4, 9, 12], 9)).toBe(9);
  expect(normalizedSelectedFileIdx([4, 9, 12], 99)).toBe(4);
  expect(normalizedSelectedFileIdx([], 9)).toBeNull();
});
