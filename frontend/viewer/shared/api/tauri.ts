export type RecipeSource = { readonly kind: "LocalRepo"; readonly value: string };
export type RecipeTarget =
  | { readonly target: "unpushed" }
  | { readonly target: "base"; readonly rev: string }
  | { readonly target: "range"; readonly range: string }
  | { readonly target: "merge"; readonly base: string }
  | { readonly target: "last"; readonly count: number };
export type RecipeOp =
  | { readonly op: "diff"; readonly target: RecipeTarget }
  | { readonly op: "merge-diff"; readonly base: string | null }
  | { readonly op: "squash-preview" };
export type Recipe = {
  readonly source: RecipeSource;
  readonly op: RecipeOp;
};

/** A named batch of recipes to open together, mirroring `gtl_recipe::OpenRecipes`. The wire
 * payload (event/invoke JSON) carries the Rust struct's literal snake_case `batch_id` field —
 * command/event payloads are plain `serde_json`, with no camelCase IPC conversion — so
 * `isOpenRecipes`/`toOpenRecipes` map it onto this camelCase shape at the boundary. */
export type OpenRecipes = {
  readonly batchId: string;
  readonly recipes: readonly Recipe[];
};

export type Commit = {
  readonly sha: string;
  readonly subject: string;
  readonly body: string;
  readonly date: string;
  readonly iso: string;
  readonly parents: readonly string[];
  readonly members: readonly string[];
  readonly is_merge: boolean;
};

export type FileSummary = {
  readonly path: string;
  readonly status: string;
  readonly added: number;
  readonly removed: number;
  readonly commits: readonly string[];
  readonly has_full: boolean;
};

export type TabMeta = {
  readonly tab_id: number;
  readonly batch_id: string;
  readonly title: string;
  readonly repo_name: string;
  readonly repo_root: string;
  readonly branch: string;
  readonly upstream: string;
  readonly cmd_lead: string;
  readonly cmd_range: string;
  readonly cmd_trail: string;
  readonly commits_label: string;
  readonly foot_cmd: string;
  readonly foot_note: string;
  readonly is_empty: boolean;
  readonly commits: readonly Commit[];
  readonly files: readonly FileSummary[];
};

export type OpenedTab = {
  readonly tab_id: number;
  readonly meta: TabMeta;
};

export type LiveViewDto = {
  readonly source_kind: string;
  readonly source_value: string;
  readonly display_name: string;
  readonly created_at: string;
  readonly last_opened_at: string | null;
};

export type SourceProbe =
  | { readonly outcome: "ok" }
  | { readonly outcome: "broken"; readonly code: string; readonly reason: string };

export type UnifiedRowKind = "meta" | "hunk" | "context" | "add" | "del";

export type UnifiedRow = {
  readonly kind: UnifiedRowKind;
  readonly old_no: number | null;
  readonly new_no: number | null;
  readonly text: string;
  readonly owner: string | null;
  readonly long_len: number | null;
};

export type Span = {
  readonly start: number;
  readonly end: number;
};

export type SplitCell = {
  readonly no: number;
  readonly text: string;
  readonly owner: string | null;
  readonly spans: readonly Span[];
  readonly long_len: number | null;
};

export type SplitRow =
  | { readonly kind: "meta"; readonly text: string }
  | { readonly kind: "hunk"; readonly text: string }
  | {
      readonly kind: "context";
      readonly old_no: number;
      readonly new_no: number;
      readonly text: string;
      readonly long_len: number | null;
    }
  | { readonly kind: "pair"; readonly old: SplitCell | null; readonly new: SplitCell | null };

export type RowsPage =
  | { readonly total: number; readonly layout: "unified"; readonly rows: readonly UnifiedRow[] }
  | { readonly total: number; readonly layout: "split"; readonly rows: readonly SplitRow[] };

export type FileRowsArgs = {
  readonly tabId: number;
  readonly fileIdx: number;
  readonly layout: "unified" | "split";
  readonly full: boolean;
  readonly start: number;
  readonly count: number;
};

type TauriCore = { readonly invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> };
type TauriEvent = {
  readonly listen: <P>(event: string, handler: (e: { readonly payload: P }) => void) => Promise<() => void>;
};
type TauriGlobal = { readonly core: TauriCore; readonly event: TauriEvent };

function record(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null ? (value as Record<string, unknown>) : undefined;
}

export function tauriGlobal(): TauriGlobal {
  const root = record(window);
  const tauri = record(root?.["__TAURI__"]);
  const core = record(tauri?.["core"]);
  const event = record(tauri?.["event"]);
  if (typeof core?.["invoke"] !== "function" || typeof event?.["listen"] !== "function") {
    throw new Error("__TAURI__ is not available; gtl-viewer must run inside a Tauri webview");
  }
  return {
    core: {
      invoke: <T>(command: string, args?: Record<string, unknown>) =>
        Promise.resolve((core["invoke"] as (c: string, a?: Record<string, unknown>) => Promise<T>)(command, args)),
    },
    event: {
      listen: <P>(name: string, handler: (e: { readonly payload: P }) => void) =>
        Promise.resolve(
          (event["listen"] as (n: string, h: (e: { readonly payload: P }) => void) => Promise<() => void>)(
            name,
            handler,
          ),
        ),
    },
  };
}

export async function drainPendingRecipes(): Promise<OpenRecipes[]> {
  const rows = await tauriGlobal().core.invoke<unknown>("drain_pending_recipes");
  return Array.isArray(rows) ? rows.filter(isOpenRecipes).map(toOpenRecipes) : [];
}

export function listenOpenRecipe(handler: (batch: OpenRecipes) => void): Promise<() => void> {
  return tauriGlobal().event.listen<unknown>("open-recipe", (e) => {
    if (isOpenRecipes(e.payload)) handler(toOpenRecipes(e.payload));
  });
}

export function isLiveViewDto(value: unknown): value is LiveViewDto {
  const row = record(value);
  if (row === undefined) return false;
  return (
    typeof row["source_kind"] === "string" &&
    typeof row["source_value"] === "string" &&
    typeof row["display_name"] === "string" &&
    typeof row["created_at"] === "string" &&
    isStringOrNull(row["last_opened_at"])
  );
}

export async function listLiveViews(): Promise<LiveViewDto[]> {
  const rows = await tauriGlobal().core.invoke<unknown>("list_live_views");
  return Array.isArray(rows) ? rows.filter(isLiveViewDto) : [];
}

export function isSourceProbe(value: unknown): value is SourceProbe {
  const row = record(value);
  if (row === undefined) return false;
  if (row["outcome"] === "ok") return true;
  if (row["outcome"] === "broken") {
    return typeof row["code"] === "string" && typeof row["reason"] === "string";
  }
  return false;
}

/** Probes a live-view source's directory before computing it, so a broken source
 * (missing dir / not a git repo) surfaces as a typed state instead of a failed diff. */
export async function probeSource(sourceKind: string, sourceValue: string): Promise<SourceProbe> {
  return invokeChecked("probe_source", isSourceProbe, { sourceKind, sourceValue });
}

function isStringArray(value: unknown): value is readonly string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isCommit(value: unknown): value is Commit {
  const row = record(value);
  if (row === undefined) return false;
  return (
    typeof row["sha"] === "string" &&
    typeof row["subject"] === "string" &&
    typeof row["body"] === "string" &&
    typeof row["date"] === "string" &&
    typeof row["iso"] === "string" &&
    isStringArray(row["parents"]) &&
    isStringArray(row["members"]) &&
    typeof row["is_merge"] === "boolean"
  );
}

function isFileSummary(value: unknown): value is FileSummary {
  const row = record(value);
  if (row === undefined) return false;
  return (
    typeof row["path"] === "string" &&
    typeof row["status"] === "string" &&
    typeof row["added"] === "number" &&
    typeof row["removed"] === "number" &&
    isStringArray(row["commits"]) &&
    typeof row["has_full"] === "boolean"
  );
}

export function isTabMeta(value: unknown): value is TabMeta {
  const row = record(value);
  if (row === undefined) return false;
  return (
    typeof row["tab_id"] === "number" &&
    typeof row["batch_id"] === "string" &&
    typeof row["title"] === "string" &&
    typeof row["repo_name"] === "string" &&
    typeof row["repo_root"] === "string" &&
    typeof row["branch"] === "string" &&
    typeof row["upstream"] === "string" &&
    typeof row["cmd_lead"] === "string" &&
    typeof row["cmd_range"] === "string" &&
    typeof row["cmd_trail"] === "string" &&
    typeof row["commits_label"] === "string" &&
    typeof row["foot_cmd"] === "string" &&
    typeof row["foot_note"] === "string" &&
    typeof row["is_empty"] === "boolean" &&
    Array.isArray(row["commits"]) &&
    row["commits"].every(isCommit) &&
    Array.isArray(row["files"]) &&
    row["files"].every(isFileSummary)
  );
}

export function isOpenedTab(value: unknown): value is OpenedTab {
  const row = record(value);
  if (row === undefined) return false;
  const meta = row["meta"];
  return typeof row["tab_id"] === "number" && isTabMeta(meta) && row["tab_id"] === meta.tab_id;
}

function isUnifiedRowKind(value: unknown): value is UnifiedRowKind {
  switch (value) {
    case "meta":
    case "hunk":
    case "context":
    case "add":
    case "del":
      return true;
    default:
      return false;
  }
}

function isUnifiedRow(value: unknown): value is UnifiedRow {
  const row = record(value);
  if (row === undefined) return false;
  return (
    isUnifiedRowKind(row["kind"]) &&
    (typeof row["old_no"] === "number" || row["old_no"] === null) &&
    (typeof row["new_no"] === "number" || row["new_no"] === null) &&
    typeof row["text"] === "string" &&
    (typeof row["owner"] === "string" || row["owner"] === null) &&
    (typeof row["long_len"] === "number" || row["long_len"] === null)
  );
}

function isSpan(value: unknown): value is Span {
  const row = record(value);
  return row !== undefined && typeof row["start"] === "number" && typeof row["end"] === "number";
}

function isSplitCell(value: unknown): value is SplitCell {
  const row = record(value);
  if (row === undefined) return false;
  return (
    typeof row["no"] === "number" &&
    typeof row["text"] === "string" &&
    (typeof row["owner"] === "string" || row["owner"] === null) &&
    Array.isArray(row["spans"]) &&
    row["spans"].every(isSpan) &&
    (typeof row["long_len"] === "number" || row["long_len"] === null)
  );
}

function isSplitRow(value: unknown): value is SplitRow {
  const row = record(value);
  if (row === undefined || typeof row["kind"] !== "string") return false;
  switch (row["kind"]) {
    case "meta":
    case "hunk":
      return typeof row["text"] === "string";
    case "context":
      return (
        typeof row["old_no"] === "number" &&
        typeof row["new_no"] === "number" &&
        typeof row["text"] === "string" &&
        (typeof row["long_len"] === "number" || row["long_len"] === null)
      );
    case "pair":
      return (row["old"] === null || isSplitCell(row["old"])) && (row["new"] === null || isSplitCell(row["new"]));
    default:
      return false;
  }
}

export function isRowsPage(value: unknown): value is RowsPage {
  const row = record(value);
  if (row === undefined || typeof row["total"] !== "number" || !Array.isArray(row["rows"])) return false;
  if (row["layout"] === "unified") return row["rows"].every(isUnifiedRow);
  if (row["layout"] === "split") return row["rows"].every(isSplitRow);
  return false;
}

function isStringOrNull(value: unknown): value is string | null {
  return typeof value === "string" || value === null;
}

async function invokeChecked<T>(
  command: string,
  guard: (value: unknown) => value is T,
  args?: Record<string, unknown>,
): Promise<T> {
  const payload = await tauriGlobal().core.invoke<unknown>(command, args);
  if (!guard(payload)) throw new Error(`Malformed ${command} response`);
  return payload;
}

function isRecipeSource(value: unknown): value is RecipeSource {
  const row = record(value);
  return row !== undefined && row["kind"] === "LocalRepo" && typeof row["value"] === "string";
}

function isRecipeTarget(value: unknown): value is RecipeTarget {
  const row = record(value);
  if (row === undefined) return false;
  switch (row["target"]) {
    case "unpushed":
      return true;
    case "base":
      return typeof row["rev"] === "string";
    case "range":
      return typeof row["range"] === "string";
    case "merge":
      return typeof row["base"] === "string";
    case "last":
      return typeof row["count"] === "number";
    default:
      return false;
  }
}

function isRecipeOp(value: unknown): value is RecipeOp {
  const row = record(value);
  if (row === undefined) return false;
  switch (row["op"]) {
    case "diff":
      return isRecipeTarget(row["target"]);
    case "merge-diff":
      return isStringOrNull(row["base"]);
    case "squash-preview":
      return true;
    default:
      return false;
  }
}

export function isRecipe(value: unknown): value is Recipe {
  const row = record(value);
  if (row === undefined) return false;
  return isRecipeSource(row["source"]) && isRecipeOp(row["op"]);
}

/** Validates the raw wire payload — the Rust struct's literal snake_case `batch_id` field,
 * with each recipe checked via {@link isRecipe} — ahead of the {@link toOpenRecipes} mapping. */
export function isOpenRecipes(
  value: unknown,
): value is { readonly batch_id: string; readonly recipes: readonly Recipe[] } {
  const row = record(value);
  if (row === undefined) return false;
  return typeof row["batch_id"] === "string" && Array.isArray(row["recipes"]) && row["recipes"].every(isRecipe);
}

function toOpenRecipes(payload: { readonly batch_id: string; readonly recipes: readonly Recipe[] }): OpenRecipes {
  return { batchId: payload.batch_id, recipes: payload.recipes };
}

export async function openRecipe(recipe: Recipe, batchId: string): Promise<OpenedTab> {
  return invokeChecked("open_recipe", isOpenedTab, { recipe, batchId });
}

export async function tabMeta(tabId: number): Promise<TabMeta> {
  return invokeChecked("tab_meta", isTabMeta, { tabId });
}

export async function fileRows(args: FileRowsArgs): Promise<RowsPage> {
  return invokeChecked("file_rows", isRowsPage, args);
}

export async function refreshTab(tabId: number): Promise<TabMeta> {
  return invokeChecked("refresh_tab", isTabMeta, { tabId });
}

function isBoolean(value: unknown): value is boolean {
  return typeof value === "boolean";
}

export async function closeNativeTab(tabId: number): Promise<boolean> {
  return invokeChecked("close_tab", isBoolean, { tabId });
}

export async function getSetting(key: string): Promise<string | null> {
  return invokeChecked("get_setting", isStringOrNull, { key });
}

export async function setSetting(key: string, value: string): Promise<void> {
  const payload = await tauriGlobal().core.invoke<unknown>("set_setting", { key, value });
  if (payload !== undefined && payload !== null) throw new Error("Malformed set_setting response");
}
