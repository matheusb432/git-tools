import { describe, expect, test } from "vitest";
import { toggleLongLine } from "./long-lines";

describe("toggleLongLine", () => {
  test("flips expanded and aria-expanded on the owning row", () => {
    const row = document.createElement("div");
    row.className = "dl-long";
    const button = document.createElement("button");
    row.appendChild(button);

    toggleLongLine(button);
    expect(row.classList.contains("expanded")).toBe(true);
    expect(button.getAttribute("aria-expanded")).toBe("true");

    toggleLongLine(button);
    expect(row.classList.contains("expanded")).toBe(false);
    expect(button.getAttribute("aria-expanded")).toBe("false");
  });
});
