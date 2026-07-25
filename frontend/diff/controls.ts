import { copyText } from "../core/clipboard";
import { extractCopyText } from "../core/copy";
import { createTeardown } from "../core/teardown";

type CopyState = "ok" | "err";

export function enhanceControls(root: HTMLElement): () => void {
  const { listen, later, destroy } = createTeardown();
  let active = true;
  const scheduleReset = (button: HTMLButtonElement, state: CopyState): void => {
    button.dataset["state"] = state;
    button.textContent = state === "ok" ? "copied" : "failed";
    later(() => {
      button.dataset["state"] = "";
      button.textContent = button.dataset["copyLabel"] ?? "copy";
    }, 1200);
  };

  root.querySelectorAll<HTMLButtonElement>(".copy-button").forEach((button) => {
    // The payload is read synchronously so copyText's execCommand fallback still runs
    // inside the click's transient user activation window.
    listen(button, "click", () => {
      void copyText(copyButtonPayload(button)).then((ok) => {
        if (active) scheduleReset(button, ok ? "ok" : "err");
      });
    });
  });
  return () => {
    active = false;
    destroy();
  };
}

function copyButtonPayload(button: HTMLButtonElement): string {
  if (button.dataset["copyMode"] === "code") {
    const file = button.closest("details.file");
    return file ? extractCopyText(file) : "";
  }
  return button.dataset["copyValue"] ?? "";
}
