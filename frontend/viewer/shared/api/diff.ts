import { z } from "zod";

export const commitSchema = z.object({
  sha: z.string(),
  subject: z.string(),
  body: z.string(),
  date: z.string(),
  iso: z.string(),
  parents: z.array(z.string()),
  members: z.array(z.string()),
  is_merge: z.boolean(),
});
export const fileSummarySchema = z.object({
  path: z.string(),
  status: z.string(),
  added: z.number().int().nonnegative(),
  removed: z.number().int().nonnegative(),
  commits: z.array(z.string()),
  has_full: z.boolean(),
});
export const tabMetaSchema = z.object({
  tab_id: z.number().int().nonnegative(),
  batch_id: z.string(),
  title: z.string(),
  repo_name: z.string(),
  repo_root: z.string(),
  branch: z.string(),
  upstream: z.string(),
  cmd_lead: z.string(),
  cmd_range: z.string(),
  cmd_trail: z.string(),
  commits_label: z.string(),
  foot_cmd: z.string(),
  foot_note: z.string(),
  is_empty: z.boolean(),
  commits: z.array(commitSchema),
  files: z.array(fileSummarySchema),
});
export const openedTabSchema = z
  .object({ tab_id: z.number().int().nonnegative(), meta: tabMetaSchema })
  .superRefine((opened, context) => {
    if (opened.tab_id !== opened.meta.tab_id) {
      context.addIssue({
        code: "custom",
        path: ["meta", "tab_id"],
        message: "meta.tab_id must match tab_id",
      });
    }
  });
export const unifiedRowSchema = z.object({
  kind: z.enum(["meta", "hunk", "context", "add", "del"]),
  old_no: z.number().int().nullable(),
  new_no: z.number().int().nullable(),
  text: z.string(),
  owner: z.string().nullable(),
  long_len: z.number().int().nonnegative().nullable(),
});
export const spanSchema = z.object({ start: z.number().int().nonnegative(), end: z.number().int().nonnegative() });
export const splitCellSchema = z.object({
  no: z.number().int().nonnegative(),
  text: z.string(),
  owner: z.string().nullable(),
  spans: z.array(spanSchema),
  long_len: z.number().int().nonnegative().nullable(),
});
export const splitRowSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("meta"), text: z.string() }),
  z.object({ kind: z.literal("hunk"), text: z.string() }),
  z.object({
    kind: z.literal("context"),
    old_no: z.number().int().nonnegative(),
    new_no: z.number().int().nonnegative(),
    text: z.string(),
    long_len: z.number().int().nonnegative().nullable(),
  }),
  z.object({ kind: z.literal("pair"), old: splitCellSchema.nullable(), new: splitCellSchema.nullable() }),
]);
export const rowsPageSchema = z.discriminatedUnion("layout", [
  z.object({ total: z.number().int().nonnegative(), layout: z.literal("unified"), rows: z.array(unifiedRowSchema) }),
  z.object({ total: z.number().int().nonnegative(), layout: z.literal("split"), rows: z.array(splitRowSchema) }),
]);
export const closeTabResultSchema = z.boolean();

export type Commit = z.infer<typeof commitSchema>;
export type FileSummary = z.infer<typeof fileSummarySchema>;
export type TabMeta = z.infer<typeof tabMetaSchema>;
export type OpenedTab = z.infer<typeof openedTabSchema>;
export type UnifiedRow = z.infer<typeof unifiedRowSchema>;
export type Span = z.infer<typeof spanSchema>;
export type SplitCell = z.infer<typeof splitCellSchema>;
export type SplitRow = z.infer<typeof splitRowSchema>;
export type RowsPage = z.infer<typeof rowsPageSchema>;
export type FileRowsInput = {
  readonly tabId: number;
  readonly fileIdx: number;
  readonly layout: "unified" | "split";
  readonly full: boolean;
  readonly start: number;
  readonly count: number;
};
