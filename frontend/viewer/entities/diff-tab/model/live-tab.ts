import type { LiveViewDto, Recipe } from "@/shared/api";
import { nativeOpeningTab, type NativeTab, type ViewerTab } from "./native-tab";

/** The saved-source identity a live view was opened from. */
export type LiveSource = {
  readonly kind: string;
  readonly value: string;
};

export type BrokenSourceCode = "DirNotFound" | "DirNotGitRepo";

/** Why a live view's source directory can no longer be diffed. */
export type BrokenSource = {
  readonly code: BrokenSourceCode;
  readonly reason: string;
};

/** A persisted live-view tab: a `NativeTab` carrying its saved source and, when the source no longer resolves, a typed error instead of a diff. */
export type LiveTab = NativeTab & {
  readonly live: true;
  readonly source: LiveSource;
  readonly displayName: string;
  readonly brokenSource?: BrokenSource;
};

function liveTabLocalId(source: LiveSource): string {
  return `live:${source.kind}:${source.value}`;
}

// * Live-view recipes are always an unpushed diff against the saved local repo — restoring or
// * retrying a live view never carries a caller-chosen target.
function recipeForLiveSource(source: LiveSource): Recipe {
  return {
    source: { kind: "LocalRepo", value: source.value },
    op: { op: "diff", target: { target: "unpushed" } },
  };
}

/** Maps a persisted row from `list_live_views` into an unopened live tab. */
export function liveTabFromDto(dto: LiveViewDto): LiveTab {
  const source: LiveSource = { kind: dto.source_kind, value: dto.source_value };
  const localId = liveTabLocalId(source);
  return {
    ...nativeOpeningTab(localId, recipeForLiveSource(source), localId),
    live: true,
    source,
    displayName: dto.display_name,
  };
}

/** Narrows a viewer tab to a live tab (a native tab carrying the `live` discriminant). */
export function isLiveTab(tab: ViewerTab): tab is LiveTab {
  return tab.kind === "native" && "live" in tab && tab.live === true;
}

/** Index of the already-open live tab for `source`, so callers can focus it instead of opening a duplicate. */
export function dedupeLiveTabBySource(tabs: readonly ViewerTab[], source: LiveSource): number | null {
  const index = tabs.findIndex(
    (tab) => isLiveTab(tab) && tab.source.kind === source.kind && tab.source.value === source.value,
  );
  return index >= 0 ? index : null;
}

/** Builds a typed broken-source state from a `save_live_view`/`probe_source` rejection, or `null` for an unrecognized code. */
export function brokenSourceFromRejection(code: string, reason: string): BrokenSource | null {
  if (code !== "DirNotFound" && code !== "DirNotGitRepo") return null;
  return { code, reason };
}

/** Marks a live tab broken (a typed probe rejection) — no compute happens, and the tab
 * lands on the shared `error` lifecycle so `TabStrip` surfaces it the same as a failed open/refresh. */
export function liveTabBroken(tab: LiveTab, brokenSource: BrokenSource): LiveTab {
  return {
    ...tab,
    brokenSource,
    lifecycle: { state: "error", message: brokenSource.reason },
  };
}
