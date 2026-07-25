import { describe, expect, test } from "vitest";
import { enhanceControls } from "./controls";

describe("enhanceControls", () => {
  test("cleanup removes button behavior before a swapped layout is discarded", async () => {
    const root = document.createElement("div");
    const button = document.createElement("button");
    button.className = "copy-button";
    button.dataset["copyValue"] = "abc123";
    button.dataset["copyLabel"] = "copy";
    root.appendChild(button);
    const cleanup = enhanceControls(root);

    button.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(button.dataset["state"]).toBe("err");

    cleanup();
    button.dataset["state"] = "";
    button.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(button.dataset["state"]).toBe("");
  });
});
