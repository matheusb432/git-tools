import { QueryClient } from "@tanstack/svelte-query";
import { render } from "@testing-library/svelte";
import type { Component, ComponentImport, Props } from "@testing-library/svelte-core/types";
import { createApiClient, type ApiTransport } from "@/shared/api";
import DataTestProvider from "./DataTestProvider.svelte";

const TEST_RESIZE_EVENT = "test:resize-observer";

export function setVirtualListGeometry(element: HTMLElement): void {
  Object.defineProperties(element, {
    clientHeight: { configurable: true, value: 600 },
    scrollHeight: { configurable: true, value: 1_200 },
  });
  element.getBoundingClientRect = () => new DOMRect(0, 0, 1_200, 600);
  element.dispatchEvent(new Event(TEST_RESIZE_EVENT));
}

export function renderWithData<C extends Component>(
  component: ComponentImport<C>,
  props: Props<C>,
  transport: ApiTransport,
) {
  const api = createApiClient(transport);
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false },
      mutations: { retry: false },
    },
  });
  const result = render(component, props, {
    wrapper: DataTestProvider,
    wrapperProps: { api, queryClient },
  });

  return { ...result, queryClient };
}
