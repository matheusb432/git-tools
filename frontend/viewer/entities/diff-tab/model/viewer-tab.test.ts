import { expect, test } from "bun:test";
import {
  closeAllViewerTabs,
  closeOtherViewerTabs,
  closeViewerTabState,
  sortViewerTabsByTime,
  viewerTabKey,
  type ArtifactTab,
  type ViewerTabState,
} from "..";

const artifact = (label: string, committedAt: string): ArtifactTab => ({
  kind: "artifact",
  url: `diff://repo/${label}`,
  label,
  committedAt,
});
const viewerState = (tabs: ViewerTabState["tabs"], active: number): ViewerTabState => ({
  tabs,
  active,
  showHistory: false,
});

test("viewerTabKey keys artifact tabs by url", () => {
  expect(viewerTabKey(artifact("a", "2026-07-07T00:00:00Z"))).toBe("artifact:diff://repo/a");
});

test("closing a tab keeps active index valid", () => {
  const state = closeViewerTabState(
    viewerState([artifact("a", "2026-07-07T00:00:00Z"), artifact("b", "2026-07-08T00:00:00Z")], 1),
    1,
  );
  expect(state.tabs).toEqual([artifact("a", "2026-07-07T00:00:00Z")]);
  expect(state.active).toBe(0);
});

test("closeOtherViewerTabs keeps only the selected tab", () => {
  const kept = artifact("b", "2026-07-08T00:00:00Z");
  const state = closeOtherViewerTabs(
    viewerState([artifact("a", "2026-07-07T00:00:00Z"), kept, artifact("c", "2026-07-06T00:00:00Z")], 0),
    1,
  );
  expect(state.tabs).toEqual([kept]);
  expect(state.active).toBe(0);
});

test("closeAllViewerTabs empties the strip", () => {
  const state = closeAllViewerTabs(
    viewerState([artifact("a", "2026-07-07T00:00:00Z"), artifact("b", "2026-07-08T00:00:00Z")], 1),
  );
  expect(state.tabs).toEqual([]);
  expect(state.active).toBe(0);
});

test("sortViewerTabsByTime orders tabs by committedAt, newest first", () => {
  const older = artifact("artifact-a", "2026-07-07T00:00:00Z");
  const newer = artifact("artifact-b", "2026-07-08T12:00:00Z");
  expect(sortViewerTabsByTime([older, newer])).toEqual([newer, older]);
});

test("sortViewerTabsByTime stays stable on ties", () => {
  const a = artifact("artifact-a", "2026-07-08T00:00:00Z");
  const b = artifact("artifact-b", "2026-07-08T00:00:00Z");
  expect(sortViewerTabsByTime([a, b])).toEqual([a, b]);
});
