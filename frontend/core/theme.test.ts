import { expect, test } from "vitest";
import { readStoredTheme, toTheme, writeStoredTheme, type StorageLike } from "./theme";

test("toTheme accepts known themes and defaults unknown values to dark", () => {
  expect(toTheme("light")).toBe("light");
  expect(toTheme("hearth")).toBe("hearth");
  expect(toTheme("unknown")).toBe("dark");
  expect(toTheme(null)).toBe("dark");
});

test("readStoredTheme returns undefined when storage has no override", () => {
  const storage: StorageLike = { getItem: () => null, setItem: () => undefined };
  expect(readStoredTheme(storage)).toBeUndefined();
});

test("writeStoredTheme stores gtl-theme", () => {
  const writes = new Map<string, string>();
  const storage: StorageLike = {
    getItem: (key) => writes.get(key) ?? null,
    setItem: (key, value) => {
      writes.set(key, value);
    },
  };
  writeStoredTheme(storage, "hearth");
  expect(writes.get("gtl-theme")).toBe("hearth");
});
