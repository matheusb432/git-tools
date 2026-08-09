import { copyText } from "./clipboard";
import { extractCopyText } from "./model/copy";
import { createTeardown } from "./teardown";

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

  listen(root, "click", (event) => {
    if (!(event.target instanceof Element)) return;
    const button = event.target.closest<HTMLButtonElement>(".copy-button");
    if (!button || !root.contains(button)) return;
    // Start the fallback inside the click's transient user activation window.
    void copyText(copyButtonPayload(button)).then((ok) => {
      if (active) scheduleReset(button, ok ? "ok" : "err");
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
