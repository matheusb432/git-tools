import { copyText } from "../core/clipboard";
import { extractCopyText } from "../core/copy";
import { toTheme, writeStoredTheme, type Theme } from "../core/theme";

type CopyState = "ok" | "err" | "";

export function enhanceControls(root: HTMLElement): void {
  enhanceTheme(root);
  root.querySelectorAll<HTMLButtonElement>(".copy-button").forEach((button) => {
    button.addEventListener("click", () => {
      void copyButtonPayload(button).then((payload) => copyText(payload)).then((ok) => {
        setCopyState(button, ok ? "ok" : "err");
      });
    });
  });
}

function enhanceTheme(root: HTMLElement): void {
  const select = root.querySelector<HTMLSelectElement>(".theme-select");
  if (!select) return;
  const current = toTheme(document.documentElement.dataset["theme"] ?? null);
  select.value = current;
  select.addEventListener("change", () => {
    const theme = toTheme(select.value);
    applyTheme(theme);
  });
}

function applyTheme(theme: Theme): void {
  document.documentElement.dataset["theme"] = theme;
  writeStoredTheme(localStorage, theme);
}

async function copyButtonPayload(button: HTMLButtonElement): Promise<string> {
  if (button.dataset["copyMode"] === "code") {
    const file = button.closest("details.file");
    return file ? extractCopyText(file) : "";
  }
  return button.dataset["copyValue"] ?? "";
}

function setCopyState(button: HTMLButtonElement, state: CopyState): void {
  button.dataset["state"] = state;
  button.textContent = state === "ok" ? "copied" : state === "err" ? "failed" : button.dataset["copyLabel"] ?? "copy";
  setTimeout(() => {
    button.dataset["state"] = "";
    button.textContent = button.dataset["copyLabel"] ?? "copy";
  }, 1200);
}
