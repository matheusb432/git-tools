import { copyText } from "../core/clipboard";
import { extractCopyText } from "../core/copy";
import { toTheme, writeStoredTheme, type Theme } from "../core/theme";

type CopyState = "ok" | "err" | "";

export function enhanceControls(root: HTMLElement): () => void {
  const cleanups: Array<() => void> = [];
  const timers = new Set<ReturnType<typeof setTimeout>>();
  let active = true;
  const scheduleReset = (button: HTMLButtonElement, state: CopyState): void => {
    button.dataset["state"] = state;
    button.textContent = state === "ok" ? "copied" : state === "err" ? "failed" : (button.dataset["copyLabel"] ?? "copy");
    const timer = setTimeout(() => {
      timers.delete(timer);
      button.dataset["state"] = "";
      button.textContent = button.dataset["copyLabel"] ?? "copy";
    }, 1200);
    timers.add(timer);
  };

  const themeCleanup = enhanceTheme(root);
  if (themeCleanup) cleanups.push(themeCleanup);
  root.querySelectorAll<HTMLButtonElement>(".copy-button").forEach((button) => {
    const handleClick = (): void => {
      void copyButtonPayload(button)
        .then((payload) => copyText(payload))
        .then((ok) => {
          if (active) scheduleReset(button, ok ? "ok" : "err");
        });
    };
    button.addEventListener("click", handleClick);
    cleanups.push(() => button.removeEventListener("click", handleClick));
  });
  return () => {
    active = false;
    cleanups.reverse().forEach((cleanup) => cleanup());
    timers.forEach(clearTimeout);
    timers.clear();
  };
}

function enhanceTheme(root: HTMLElement): (() => void) | undefined {
  const select = root.querySelector<HTMLSelectElement>(".theme-select");
  if (!select) return;
  const current = toTheme(document.documentElement.dataset["theme"] ?? null);
  select.value = current;
  const handleChange = (): void => {
    const theme = toTheme(select.value);
    applyTheme(theme);
  };
  select.addEventListener("change", handleChange);
  return () => select.removeEventListener("change", handleChange);
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
