import { describe, expect, test } from "vitest";
import { installThemeControl, THEME_INPUT_ATTRIBUTE } from "./theme-control";

describe("installThemeControl", () => {
  function changeInput(themeValue: string | null): void {
    const input = document.createElement("input");
    if (themeValue !== null) input.setAttribute(THEME_INPUT_ATTRIBUTE, themeValue);
    document.body.appendChild(input);
    installThemeControl(document);
    input.dispatchEvent(new Event("change", { bubbles: true }));
  }

  test("a theme radio forwards its value while a valueless input leaves the theme alone", () => {
    changeInput("dark");
    expect(document.documentElement.dataset["theme"]).toBe("dark");

    changeInput(null);
    expect(document.documentElement.dataset["theme"]).toBe("dark");
  });

  // Mirrors theme.rs's rendered shape: label > input + wrapper span > (swatch
  // span, name span), so the "last span in the label" extraction is exercised
  // against the same nesting it enhances in the real viewer.
  test("picking a theme radio mirrors the option's rendered name onto the trigger", () => {
    const trigger = document.createElement("span");
    trigger.id = "viewer-theme-name";
    trigger.textContent = "Dark";
    document.body.appendChild(trigger);
    const label = document.createElement("label");
    const hearth = document.createElement("input");
    hearth.type = "radio";
    hearth.setAttribute(THEME_INPUT_ATTRIBUTE, "hearth");
    const wrapper = document.createElement("span");
    const swatch = document.createElement("span");
    const name = document.createElement("span");
    name.textContent = "Hearth";
    wrapper.appendChild(swatch);
    wrapper.appendChild(name);
    label.appendChild(hearth);
    label.appendChild(wrapper);
    document.body.appendChild(label);

    installThemeControl(document);
    hearth.dispatchEvent(new Event("change", { bubbles: true }));

    expect(trigger.textContent).toBe("Hearth");
  });
});
