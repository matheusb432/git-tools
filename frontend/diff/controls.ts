import { copyText } from "../core/clipboard";
import { extractCopyText } from "../core/copy";

type CopyState = "ok" | "err" | "";

export function enhanceControls(root: HTMLElement): () => void {
  const cleanups: Array<() => void> = [];
  const timers = new Set<ReturnType<typeof setTimeout>>();
  let active = true;
  const scheduleReset = (button: HTMLButtonElement, state: CopyState): void => {
    button.dataset["state"] = state;
    button.textContent =
      state === "ok" ? "copied" : state === "err" ? "failed" : (button.dataset["copyLabel"] ?? "copy");
    const timer = setTimeout(() => {
      timers.delete(timer);
      button.dataset["state"] = "";
      button.textContent = button.dataset["copyLabel"] ?? "copy";
    }, 1200);
    timers.add(timer);
  };

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

async function copyButtonPayload(button: HTMLButtonElement): Promise<string> {
  if (button.dataset["copyMode"] === "code") {
    const file = button.closest("details.file");
    return file ? extractCopyText(file) : "";
  }
  return button.dataset["copyValue"] ?? "";
}
