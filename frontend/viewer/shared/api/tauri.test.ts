import { expect, test } from "bun:test";
import { createTauriTransport } from "./tauri";

function setWindow(value: unknown): void {
  Reflect.set(globalThis, "window", value);
}

test("createTauriTransport throws when __TAURI__ is absent", () => {
  setWindow({});
  expect(() => createTauriTransport()).toThrow("__TAURI__ is not available");
});

test("transport forwards generic operations and event payloads", async () => {
  const calls: Array<{ readonly operation: string; readonly input: unknown }> = [];
  const payloads: unknown[] = [];
  setWindow({
    __TAURI__: {
      core: {
        invoke: async (operation: string, input: unknown) => {
          calls.push({ operation, input });
          return { ok: true };
        },
      },
      event: {
        listen: async (_event: string, handler: (event: { readonly payload: unknown }) => void) => {
          handler({ payload: { batch_id: "batch-1" } });
          return () => undefined;
        },
      },
    },
  });
  const transport = createTauriTransport();

  await expect(transport.invoke("file_rows", { tabId: 7 })).resolves.toEqual({ ok: true });
  await transport.listen("open-recipe", (payload) => payloads.push(payload));

  expect(calls).toEqual([{ operation: "file_rows", input: { tabId: 7 } }]);
  expect(payloads).toEqual([{ batch_id: "batch-1" }]);
});
