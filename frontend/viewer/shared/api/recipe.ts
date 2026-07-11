import { z } from "zod";

export const recipeSourceSchema = z.object({ kind: z.literal("LocalRepo"), value: z.string() });
export const recipeTargetSchema = z.discriminatedUnion("target", [
  z.object({ target: z.literal("unpushed") }),
  z.object({ target: z.literal("base"), rev: z.string() }),
  z.object({ target: z.literal("range"), range: z.string() }),
  z.object({ target: z.literal("merge"), base: z.string() }),
  z.object({ target: z.literal("last"), count: z.number().int().nonnegative() }),
]);
export const recipeOpSchema = z.discriminatedUnion("op", [
  z.object({ op: z.literal("diff"), target: recipeTargetSchema }),
  z.object({ op: z.literal("merge-diff"), base: z.string().nullable() }),
  z.object({ op: z.literal("squash-preview") }),
]);
export const recipeSchema = z.object({ source: recipeSourceSchema, op: recipeOpSchema });
export const openRecipesWireSchema = z
  .object({ batch_id: z.string(), recipes: z.array(recipeSchema) })
  .transform(({ batch_id, recipes }) => ({ batchId: batch_id, recipes }));
export const openRecipeBatchesSchema = z.array(openRecipesWireSchema);

export type RecipeSource = z.infer<typeof recipeSourceSchema>;
export type RecipeTarget = z.infer<typeof recipeTargetSchema>;
export type RecipeOp = z.infer<typeof recipeOpSchema>;
export type Recipe = z.infer<typeof recipeSchema>;
export type OpenRecipes = z.infer<typeof openRecipesWireSchema>;
