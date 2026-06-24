import { expect, test } from "bun:test";
import { copyText } from "./clipboard";

test("copyText resolves true via execCommand fallback when clipboard API absent", async () => {
  // @ts-expect-error stub
  globalThis.navigator = {};
  const created: unknown[] = [];
  // @ts-expect-error stub
  globalThis.document = {
    createElement: () => { const el = { value: "", style: {}, select() {} }; created.push(el); return el; },
    body: { appendChild() {}, removeChild() {} },
    execCommand: () => true,
  };
  expect(await copyText("hi")).toBe(true);
  expect((created[0] as { value: string }).value).toBe("hi");
});
