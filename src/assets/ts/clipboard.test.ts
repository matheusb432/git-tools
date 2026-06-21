import { expect, test } from "bun:test";
import { copyText } from "./clipboard";

test("copyText resolves true via execCommand fallback when clipboard API absent", async () => {
  // @ts-expect-error stub
  globalThis.navigator = {};
  const created: any[] = [];
  // @ts-expect-error stub
  globalThis.document = {
    createElement: () => { const el: any = { style: {}, select() {} }; created.push(el); return el; },
    body: { appendChild() {}, removeChild() {} },
    execCommand: () => true,
  };
  expect(await copyText("hi")).toBe(true);
  expect(created[0].value).toBe("hi");
});
