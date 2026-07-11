import { expect, test } from "bun:test";
import { z } from "zod";
import { createApiClient, type ApiTransport } from "./client";

test("generic query decodes through the caller schema", async () => {
  const transport: ApiTransport = {
    invoke: async () => ({ value: 7 }),
    listen: async () => () => undefined,
  };
  const api = createApiClient(transport);

  await expect(api.query("counter", {}, z.object({ value: z.number() }))).resolves.toEqual({ value: 7 });
});

test("generic query reports operation and schema path", async () => {
  const transport: ApiTransport = {
    invoke: async () => ({ value: "seven" }),
    listen: async () => () => undefined,
  };
  const api = createApiClient(transport);

  await expect(api.query("counter", {}, z.object({ value: z.number() }))).rejects.toThrow(/counter.*value/s);
});

test("an already-aborted query skips transport invocation", async () => {
  const controller = new AbortController();
  controller.abort();
  let invoked = false;
  const transport: ApiTransport = {
    invoke: async () => {
      invoked = true;
      return { value: 7 };
    },
    listen: async () => () => undefined,
  };
  const api = createApiClient(transport);

  await expect(api.query("counter", {}, z.object({ value: z.number() }), controller.signal)).rejects.toMatchObject({
    name: "AbortError",
  });
  expect(invoked).toBe(false);
});

test("an aborted query discards a late transport result", async () => {
  const controller = new AbortController();
  const transport: ApiTransport = {
    invoke: async () => {
      controller.abort();
      return { value: 7 };
    },
    listen: async () => () => undefined,
  };
  const api = createApiClient(transport);

  await expect(api.query("counter", {}, z.object({ value: z.number() }), controller.signal)).rejects.toMatchObject({
    name: "AbortError",
  });
});
