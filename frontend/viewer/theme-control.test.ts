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

  // Mirrors theme.rs's rendered shape: label > input + wrapper span > (swatch
  // span, name span), so the "last span in the label" extraction is exercised
  // against the same nesting it enhances in the real viewer.
  function appendChoice(theme: string, name: string, autofocus: boolean): HTMLInputElement {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "radio";
    input.setAttribute(THEME_INPUT_ATTRIBUTE, theme);
    if (autofocus) input.setAttribute("autofocus", "");
    const wrapper = document.createElement("span");
    const swatch = document.createElement("span");
    const nameSpan = document.createElement("span");
    nameSpan.textContent = name;
    wrapper.appendChild(swatch);
    wrapper.appendChild(nameSpan);
    label.appendChild(input);
    label.appendChild(wrapper);
    document.body.appendChild(label);
    return input;
  }

  test("picking a theme radio mirrors the option's rendered name onto the trigger", () => {
    const trigger = document.createElement("span");
    trigger.id = "viewer-theme-name";
    trigger.textContent = "Dark";
    document.body.appendChild(trigger);
    const hearth = appendChoice("hearth", "Hearth", false);

    installThemeControl(document);
    hearth.dispatchEvent(new Event("change", { bubbles: true }));

    expect(trigger.textContent).toBe("Hearth");

    trigger.remove();
    hearth.closest("label")?.remove();
  });

  test("picking a theme radio moves autofocus onto the picked radio", () => {
    const dark = appendChoice("dark", "Dark", true);
    const hearth = appendChoice("hearth", "Hearth", false);

    installThemeControl(document);
    hearth.dispatchEvent(new Event("change", { bubbles: true }));

    expect(dark.getAttribute("autofocus")).toBeNull();
    expect(hearth.getAttribute("autofocus")).not.toBeNull();

    dark.closest("label")?.remove();
    hearth.closest("label")?.remove();
  });
});
