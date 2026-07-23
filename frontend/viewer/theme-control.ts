export const THEME_INPUT_ATTRIBUTE = "data-viewer-theme";

const installedDocuments = new WeakSet<Document>();

// Theme radios are server-rendered and swapped with the settings fragment, so one
// document-level listener outlives every swap. Presentation stays in CSS keyed off
// the root data-theme attribute; this only forwards the picked value.
export function installThemeControl(root: Document): void {
  if (installedDocuments.has(root)) return;
  installedDocuments.add(root);
  root.addEventListener("change", (event) => {
    const input = event.target;
    if (!(input instanceof HTMLInputElement)) return;
    const theme = input.getAttribute(THEME_INPUT_ATTRIBUTE);
    if (theme) root.documentElement.dataset["theme"] = theme;
  });
}
