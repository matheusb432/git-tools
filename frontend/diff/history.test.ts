import { describe, expect, test } from "vitest";
import { classifyHistoryClick, installHistoryActions } from "./history";

describe("classifyHistoryClick", () => {
  test("a copy button yields its stored payload", () => {
    const button = document.createElement("button");
    button.setAttribute("data-history-copy", '{"id":9}');
    expect(classifyHistoryClick(button, null, false)).toEqual({ kind: "copy", payload: '{"id":9}' });
  });

  test("a row click opens unless a live selection ends inside the row", () => {
    const row = document.createElement("div");

    expect(classifyHistoryClick(null, row, false)).toEqual({ kind: "open" });
    expect(classifyHistoryClick(null, row, true)).toEqual({ kind: "select" });
  });

  test("a click outside any row is ignored", () => {
    expect(classifyHistoryClick(null, null, false)).toEqual({ kind: "ignore" });
  });
});

describe("installHistoryActions", () => {
  function historyRow(): { row: HTMLElement; copyButton: HTMLElement } {
    const row = document.createElement("div");
    row.className = "viewer-history-row";
    const copyButton = document.createElement("button");
    copyButton.setAttribute("data-history-copy", '{"id":9}');
    row.appendChild(copyButton);
    document.body.appendChild(row);
    installHistoryActions(document);
    return { row, copyButton };
  }

  test("clicking the copy button writes the row's payload to the clipboard", () => {
    const writes: string[] = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: (text: string): Promise<void> => (writes.push(text), Promise.resolve()) },
    });
    const { copyButton } = historyRow();

    copyButton.dispatchEvent(new Event("click", { bubbles: true }));

    expect(writes).toEqual(['{"id":9}']);
  });

  test("copying never reaches the row's bubbling open handler", () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: (): Promise<void> => Promise.resolve() },
    });
    const { row, copyButton } = historyRow();
    let opens = 0;
    row.addEventListener("click", () => (opens += 1));

    copyButton.dispatchEvent(new Event("click", { bubbles: true }));

    expect(opens).toBe(0);
  });

  test("Enter on a focused row activates it", () => {
    const { row } = historyRow();
    let clicks = 0;
    row.addEventListener("click", () => (clicks += 1));

    const keydown = new Event("keydown", { bubbles: true });
    Object.defineProperty(keydown, "key", { value: "Enter" });
    row.dispatchEvent(keydown);

    expect(clicks).toBe(1);
  });
});
