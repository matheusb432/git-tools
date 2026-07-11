import { z } from "zod";

export const settingValueSchema = z.string().nullable();
export const unitSchema = z.null().transform(() => undefined);
