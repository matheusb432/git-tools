import type { ZodType } from "zod";

export type ApiTransport = {
  readonly invoke: (operation: string, input?: Record<string, unknown>, signal?: AbortSignal) => Promise<unknown>;
  readonly listen: (event: string, receive: (payload: unknown) => void) => Promise<() => void>;
};

export type ApiClient = {
  readonly query: <Output>(
    operation: string,
    input: Record<string, unknown>,
    schema: ZodType<Output>,
    signal?: AbortSignal,
  ) => Promise<Output>;
  readonly mutate: <Output>(
    operation: string,
    input: Record<string, unknown>,
    schema: ZodType<Output>,
  ) => Promise<Output>;
  readonly listen: <Output>(
    event: string,
    schema: ZodType<Output>,
    receive: (payload: Output) => void,
  ) => Promise<() => void>;
};

function abortError(): DOMException {
  return new DOMException("The operation was aborted", "AbortError");
}

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted === true) throw abortError();
}

export function createApiClient(transport: ApiTransport): ApiClient {
  const decode = <Output>(operation: string, schema: ZodType<Output>, raw: unknown): Output => {
    const parsed = schema.safeParse(raw);
    if (!parsed.success) {
      throw new Error(`Malformed ${operation} response: ${JSON.stringify(parsed.error.issues)}`);
    }
    return parsed.data;
  };
  const invoke = async <Output>(
    operation: string,
    input: Record<string, unknown>,
    schema: ZodType<Output>,
    signal?: AbortSignal,
  ): Promise<Output> => {
    throwIfAborted(signal);
    const raw = await transport.invoke(operation, input, signal);
    throwIfAborted(signal);
    return decode(operation, schema, raw);
  };

  return {
    query: invoke,
    mutate: (operation, input, schema) => invoke(operation, input, schema),
    listen: (event, schema, receive) => transport.listen(event, (raw) => receive(decode(event, schema, raw))),
  };
}
