import { describe, expect, test } from "vitest";
import { installThemeControl, THEME_INPUT_ATTRIBUTE } from "./theme-control";

describe("installThemeControl", () => {
  function changeInput(themeValue: string | null): void {
    const input = document.createElement("input");
    if (themeValue !== null) input.setAttribute(THEME_INPUT_ATTRIBUTE, themeValue);
    document.body.appendChild(input);
    installThemeControl(document);
    input.dispatchEvent(new Event("change", { bubbles: true }));
    input.remove();
  }

  test("picking a theme radio forwards its value to the document root", () => {
    changeInput("dark");

    expect(document.documentElement.dataset["theme"]).toBe("dark");
  });

  test("inputs without a theme value leave the document theme alone", () => {
    document.documentElement.dataset["theme"] = "sepia";

    changeInput(null);

    expect(document.documentElement.dataset["theme"]).toBe("sepia");
  });
});
