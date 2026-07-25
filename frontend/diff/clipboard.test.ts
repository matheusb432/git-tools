import { afterEach, expect, test, vi } from "vitest";
import { copyText } from "./clipboard";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

test("copyText resolves true via execCommand fallback when clipboard API absent", async () => {
  vi.stubGlobal("navigator", {});
  let copied: string | null = null;
  vi.spyOn(document, "execCommand").mockImplementation(() => {
    copied = document.body.querySelector("textarea")?.value ?? null;
    return true;
  });

  expect(await copyText("hi")).toBe(true);
  expect(copied).toBe("hi");
  expect(document.body.querySelector("textarea")).toBeNull();
});
