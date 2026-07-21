/// <reference types="node" />
// The inline sources ship verbatim as <script> bodies. These tests pin the
// contract the copy step relies on: the committed generated snippets are
// byte-identical to their sources, and every source parses as plain
// JavaScript (a TS-only annotation would reach browsers as a syntax error).
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const SNIPPETS = [
  ["frontend/inline/theme-boot.ts", "crates/preview/src/embedded/generated/boot.js"],
  ["frontend/inline/theme-control.ts", "crates/desktop/src/embedded/generated/theme-control.js"],
  ["frontend/inline/pending-recipes.ts", "crates/desktop/src/embedded/generated/pending-recipes.js"],
] as const;

describe("server-inlined scripts", () => {
  for (const [source, generated] of SNIPPETS) {
    it(`${source} is valid plain JavaScript`, () => {
      const body = readFileSync(source, "utf8");
      expect(() => new Function(body)).not.toThrow();
      expect(body).not.toContain("\n");
    });

    it(`${generated} matches its source byte for byte`, () => {
      expect(readFileSync(generated, "utf8")).toBe(readFileSync(source, "utf8"));
    });
  }

  it("theme boot does not read or write layout and density preferences", () => {
    const body = readFileSync("frontend/inline/theme-boot.ts", "utf8");

    expect(body).not.toContain("gtl-diff-layout");
    expect(body).not.toContain("diffLayout");
    expect(body).not.toContain("diffFull");
  });
});
