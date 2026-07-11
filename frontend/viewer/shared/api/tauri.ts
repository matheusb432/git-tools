import type { ApiTransport } from "./client";

type TauriCore = {
  readonly invoke: (operation: string, input?: Record<string, unknown>) => Promise<unknown>;
};
type TauriEvent = {
  readonly listen: (event: string, handler: (event: { readonly payload: unknown }) => void) => Promise<() => void>;
};
type TauriGlobal = { readonly core: TauriCore; readonly event: TauriEvent };

function isTauriGlobal(value: unknown): value is TauriGlobal {
  if (typeof value !== "object" || value === null) return false;
  const core = Reflect.get(value, "core");
  const event = Reflect.get(value, "event");
  return (
    typeof core === "object" &&
    core !== null &&
    typeof Reflect.get(core, "invoke") === "function" &&
    typeof event === "object" &&
    event !== null &&
    typeof Reflect.get(event, "listen") === "function"
  );
}

function discoverTauri(): TauriGlobal {
  const tauri = Reflect.get(window, "__TAURI__");
  if (!isTauriGlobal(tauri)) {
    throw new Error("__TAURI__ is not available; gtl-viewer must run inside a Tauri webview");
  }
  return tauri;
}

export function createTauriTransport(): ApiTransport {
  const tauri = discoverTauri();
  return {
    invoke: (operation, input) => tauri.core.invoke(operation, input),
    listen: (event, receive) => tauri.event.listen(event, ({ payload }) => receive(payload)),
  };
}
