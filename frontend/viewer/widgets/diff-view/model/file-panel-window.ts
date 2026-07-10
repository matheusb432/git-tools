export const FILE_PANEL_WINDOW_TARGET = 10;
export const FILE_PANEL_OVERSCAN = 2;
export const FILE_PANEL_ESTIMATED_HEIGHT = 280;

export type FilePanelIdentity = {
  readonly fileIdx: number;
  readonly path: string;
};

export function filePanelDomKey(file: FilePanelIdentity): string {
  return `${file.fileIdx}:${file.path}`;
}

export function filePanelMountedBudget(
  windowTarget: number = FILE_PANEL_WINDOW_TARGET,
  overscan: number = FILE_PANEL_OVERSCAN,
): number {
  return Math.max(1, Math.trunc(windowTarget)) + Math.max(0, Math.trunc(overscan)) * 2;
}

export function filePanelOverscanForViewport(
  viewportHeight: number,
  windowTarget: number = FILE_PANEL_WINDOW_TARGET,
  minimumOverscan: number = FILE_PANEL_OVERSCAN,
  estimatedHeight: number = FILE_PANEL_ESTIMATED_HEIGHT,
): number {
  const normalizedTarget = Math.max(1, Math.trunc(windowTarget));
  const normalizedMinimum = Math.max(0, Math.trunc(minimumOverscan));
  const normalizedHeight = Math.max(1, Math.trunc(estimatedHeight));
  const visiblePanels = Math.max(1, Math.ceil(Math.max(0, viewportHeight) / normalizedHeight));
  return Math.max(normalizedMinimum, Math.ceil(Math.max(0, normalizedTarget - visiblePanels) / 2));
}

export function normalizedSelectedFileIdx(
  visibleFileIndexes: readonly number[],
  selectedFileIdx: number | null,
): number | null {
  if (visibleFileIndexes.length === 0) return null;
  if (selectedFileIdx !== null && visibleFileIndexes.includes(selectedFileIdx)) return selectedFileIdx;
  return visibleFileIndexes[0] ?? null;
}
