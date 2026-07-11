import type { LiveViewDto, Recipe, RecipeSource } from "@/shared/api";
import type { TabShell } from "./viewer-session";

export type BrokenSourceCode = "DirNotFound" | "DirNotGitRepo";

/** Why a live view's source directory can no longer be diffed. */
export type BrokenSource = {
  readonly code: BrokenSourceCode;
  readonly reason: string;
};

function liveTabLocalId(source: RecipeSource): string {
  return `live:${source.kind}:${source.value}`;
}

function recipeForLiveSource(source: RecipeSource): Recipe {
  return {
    source,
    op: { op: "diff", target: { target: "unpushed" } },
  };
}

/** Maps a persisted live-view row into an unopened durable session shell. */
export function liveTabFromDto(dto: LiveViewDto): TabShell {
  if (dto.source_kind !== "LocalRepo") {
    throw new Error(`Unsupported live-view source kind: ${dto.source_kind}`);
  }
  const source: RecipeSource = { kind: "LocalRepo", value: dto.source_value };
  const localId = liveTabLocalId(source);
  return {
    localId,
    recipe: recipeForLiveSource(source),
    batchId: localId,
    tabId: null,
    isNew: false,
    failure: null,
    live: {
      source,
      displayName: dto.display_name,
      needsActivation: true,
      brokenSource: null,
    },
  };
}

/** Builds a typed broken-source state from a source-probe rejection, or `null` for an unrecognized code. */
export function brokenSourceFromRejection(code: string, reason: string): BrokenSource | null {
  if (code !== "DirNotFound" && code !== "DirNotGitRepo") return null;
  return { code, reason };
}
