import { describe, expect, test } from "vitest";
import { applyStoredTheme, THEME_STORAGE_KEY } from "./theme-boot";

describe("applyStoredTheme", () => {
  test("applies only the stored theme, never another preference", () => {
    const dataset: DOMStringMap = {};

    applyStoredTheme({ getItem: (key) => (key === THEME_STORAGE_KEY ? "dark" : "unexpected") }, { dataset });

    expect(dataset).toEqual({ theme: "dark" });
  });

  test("leaves the server-rendered default when nothing is stored", () => {
    const dataset: DOMStringMap = {};

    applyStoredTheme({ getItem: () => null }, { dataset });

    expect(dataset).toEqual({});
  });
});
