import { expect, test } from "bun:test";
import { copyText } from "./clipboard";

test("copyText resolves true via execCommand fallback when clipboard API absent", async () => {
  // ! Swap in a minimal document/navigator for the fallback path, but restore the shared
  // ! dom-stub globals afterward — bun runs every test file in one process, so leaving these
  // ! overwritten pollutes any test file that runs later and expects the fuller stub.
  const previousDocument = globalThis.document;
  const previousNavigator = globalThis.navigator;
  try {
    globalThis.navigator = {} as unknown as Navigator;
    const created: unknown[] = [];
    globalThis.document = {
      createElement: () => { const el = { value: "", style: {}, select() {} }; created.push(el); return el; },
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
