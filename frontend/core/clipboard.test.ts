import { expect, test } from "bun:test";
import { copyText } from "./clipboard";

test("copyText resolves true via execCommand fallback when clipboard API absent", async () => {
  globalThis.navigator = {} as unknown as Navigator;
  const created: unknown[] = [];
  globalThis.document = {
    createElement: () => { const el = { value: "", style: {}, select() {} }; created.push(el); return el; },
    body: { appendChild() {}, removeChild() {} },
    execCommand: () => true,
  } as unknown as Document;
  expect(await copyText("hi")).toBe(true);
  expect((created[0] as { value: string }).value).toBe("hi");
});
