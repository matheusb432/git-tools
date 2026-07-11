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

test("a listener reports malformed event payloads through its typed decode-error boundary", async () => {
  let receiveRaw: ((payload: unknown) => void) | undefined;
  const errors: Error[] = [];
  const received: number[] = [];
  const transport: ApiTransport = {
    invoke: async () => undefined,
    listen: async (_event, receive) => {
      receiveRaw = receive;
      return () => undefined;
    },
  };
  const api = createApiClient(transport);
  await api.listen("counter-changed", z.object({ value: z.number() }), (payload) => received.push(payload.value), (error) =>
    errors.push(error),
  );

  receiveRaw?.({ value: "seven" });

  expect(received).toEqual([]);
  expect(errors).toHaveLength(1);
  expect(errors[0]?.message).toMatch(/counter-changed.*value/s);
});

test("a malformed listener payload still throws when no decode-error handler is provided", async () => {
  let receiveRaw: ((payload: unknown) => void) | undefined;
  const transport: ApiTransport = {
    invoke: async () => undefined,
    listen: async (_event, receive) => {
      receiveRaw = receive;
      return () => undefined;
    },
  };
  const api = createApiClient(transport);
  await api.listen("counter-changed", z.object({ value: z.number() }), () => undefined);

  expect(() => receiveRaw?.({ value: "seven" })).toThrow(/counter-changed.*value/s);
});
