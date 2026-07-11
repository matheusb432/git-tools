import { createContext } from "svelte";
import type { ApiClient } from "./client";

const [getApiContext, setApiContext] = createContext<ApiClient>();

export function provideApi(api: ApiClient): void {
  setApiContext(api);
}

export function useApi(): ApiClient {
  try {
    return getApiContext();
  } catch {
    throw new Error("Viewer API provider is missing");
  }
}
