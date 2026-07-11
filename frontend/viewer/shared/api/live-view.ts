import { z } from "zod";

export const liveViewSchema = z.object({
  source_kind: z.string(),
  source_value: z.string(),
  display_name: z.string(),
  created_at: z.string(),
  last_opened_at: z.string().nullable(),
});
export const liveViewsSchema = z.array(liveViewSchema);
export const sourceProbeSchema = z.discriminatedUnion("outcome", [
  z.object({ outcome: z.literal("ok") }),
  z.object({ outcome: z.literal("broken"), code: z.string(), reason: z.string() }),
]);

export type LiveView = z.infer<typeof liveViewSchema>;
export type LiveViewDto = LiveView;
export type SourceProbe = z.infer<typeof sourceProbeSchema>;
