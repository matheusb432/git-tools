import type { HistoryEntry } from "../core/history";
import type { Tab } from "../core/tabs";

export type { HistoryEntry, Tab };

export type TauriCore = { readonly invoke: <T>(command: string) => Promise<T> };
export type TauriEvent = {
  readonly listen: <P>(event: string, handler: (e: { readonly payload: P }) => void) => Promise<() => void>;
};
export type TauriGlobal = { readonly core: TauriCore; readonly event: TauriEvent };
