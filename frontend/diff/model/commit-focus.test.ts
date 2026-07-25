import { describe, expect, test } from "vitest";
import { resolveActiveSet } from "./commit-focus";

function must<T>(value: T | null, what: string): T {
  if (value === null) throw new Error(`${what} is missing`);
  return value;
}

describe("resolveActiveSet", () => {
  test("a non-merge card focuses its own sha", () => {
    const focus = must(resolveActiveSet("abc123def", "", null), "the commit focus");

    expect(focus.sha).toBe("abc123def");
    expect([...focus.shas]).toEqual(["abc123def"]);
  });

  test("a merge card focuses its member list", () => {
    const focus = must(resolveActiveSet("merge1234", "aaa111aaa bbb222bbb", null), "the commit focus");

    expect(focus.sha).toBe("merge1234");
    expect([...focus.shas]).toEqual(["aaa111aaa", "bbb222bbb"]);
  });

  test("clicking the focused sha clears the focus", () => {
    expect(resolveActiveSet("abc123def", "", "abc123def")).toBeNull();
  });
});
