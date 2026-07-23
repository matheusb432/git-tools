import { expect, test } from "vitest";
import { copyText } from "./clipboard";

test("copyText resolves true via execCommand fallback when clipboard API absent", async () => {
  // Swap in a minimal document/navigator for the fallback path and restore the shared
  // dom-stub globals afterward, so a reused runner process keeps the fuller stub.
  const previousDocument = globalThis.document;
  const previousNavigator = globalThis.navigator;
  try {
    globalThis.navigator = {} as unknown as Navigator;
    const created: unknown[] = [];
    globalThis.document = {
      createElement: () => {
        const el = { value: "", style: {}, select() {} };
        created.push(el);
        return el;
      },
      body: { appendChild() {}, removeChild() {} },
      execCommand: () => true,
    } as unknown as Document;
    expect(await copyText("hi")).toBe(true);
    expect((created[0] as { value: string }).value).toBe("hi");
  } finally {
    globalThis.document = previousDocument;
    globalThis.navigator = previousNavigator;
  }
});
