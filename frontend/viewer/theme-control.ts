import { installOnce } from "../shared/install-once";

export const THEME_INPUT_ATTRIBUTE = "data-viewer-theme";

// The picked radio's option row carries the palette's rendered name as the last
// span in its label; reading it back keeps the trigger copy server-authored
// instead of building a name from the theme value.
function optionName(input: HTMLInputElement): string | undefined {
  const spans = input.closest("label")?.querySelectorAll("span");
  return spans?.[spans.length - 1]?.textContent ?? undefined;
}

// Theme radios are server-rendered and swapped with the settings fragment, so one
// document-level listener outlives every swap. Presentation stays in CSS keyed off
// the root data-theme attribute; this only forwards the picked value. The panel's
// hx-swap="none" pick also leaves the trigger name and the panel's autofocus target
// stale, since both are rendered once per document load and never swapped -- this
// mirrors the picked row's server-rendered name and autofocus state onto both.
export const installThemeControl = installOnce((root: Document): void => {
  root.addEventListener("change", (event) => {
    const input = event.target;
    if (!(input instanceof HTMLInputElement)) return;
    const theme = input.getAttribute(THEME_INPUT_ATTRIBUTE);
    if (!theme) return;
    root.documentElement.dataset["theme"] = theme;

    const trigger = root.getElementById("viewer-theme-name");
    const name = optionName(input);
    if (trigger && name) trigger.textContent = name;

    for (const previous of root.querySelectorAll<HTMLInputElement>(`[${THEME_INPUT_ATTRIBUTE}][autofocus]`)) {
      previous.removeAttribute("autofocus");
    }
    input.setAttribute("autofocus", "");
  });
});
