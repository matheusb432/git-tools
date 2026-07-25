import { describe, expect, test } from "vitest";
import { buildFileLeaf } from "./file-tree";

function must<T>(value: T | null, what: string): T {
  if (value === null) throw new Error(`${what} is missing`);
  return value;
}

describe("buildFileLeaf", () => {
  function leaf(): HTMLElement {
    return buildFileLeaf(document, {
      name: "main.ts",
      status: "added",
      statusCode: "A",
      statusLabel: "Added file",
      el: { id: "file-main-ts" },
    });
  }

  test("labels the leaf with the name first and the status badge last", () => {
    const parts = [...leaf().querySelectorAll(".tname, .tstatus")];

    expect(parts.map((part) => part.textContent)).toEqual(["main.ts", "A"]);
    expect(parts[1]?.getAttribute("aria-label")).toBe("Added file");
  });

  test("emits semantic hooks without presentation utilities", () => {
    const li = leaf();

    expect(li.classList.contains("tfile")).toBe(true);
    expect(li.classList.contains("status-added")).toBe(true);
    expect(li.getAttribute("data-target")).toBe("file-main-ts");
    expect(must(li.querySelector<HTMLElement>(".tlabel"), "the leaf label").className).toBe("tlabel");
    expect(must(li.querySelector<HTMLElement>(".tname"), "the leaf name").className).toBe("tname");
    expect(must(li.querySelector<HTMLElement>(".tstatus"), "the leaf status").className).toBe("tstatus status-added");
  });
});
