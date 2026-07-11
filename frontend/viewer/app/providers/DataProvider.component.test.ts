import { render, screen } from "@testing-library/svelte";
import { afterEach, expect, test } from "vitest";
import DataProviderHarness from "./DataProviderHarness.svelte";
import ProviderProbe from "./ProviderProbe.svelte";

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI__");
});

test("provides the generic API and exact root query defaults to descendants", async () => {
  Reflect.set(window, "__TAURI__", {
    core: { invoke: async () => ({ value: 7 }) },
    event: { listen: async () => () => undefined },
  });

  render(DataProviderHarness);

  await screen.findByText("7");
  const probe = screen.getByTestId("data-provider-probe");
  expect(probe).toHaveTextContent("7");
  expect(probe).toHaveAttribute("data-query-retry", "false");
  expect(probe).toHaveAttribute("data-refetch-on-window-focus", "false");
  expect(probe).toHaveAttribute("data-mutation-retry", "false");
});

test("typed API context reports a missing provider", () => {
  expect(() => render(ProviderProbe)).toThrow("Viewer API provider is missing");
});
