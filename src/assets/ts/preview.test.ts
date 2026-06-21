import { expect, test } from "bun:test";
import { resolveActiveSet } from "./preview";

// JS behavior contracts migrated from deleted Rust PREVIEW_JS.contains tests.
// Covered here so bun test owns the JS logic while cargo test stays bun-free.

test("non-merge card: active set is [sha]", () => {
  const result = resolveActiveSet("abc123def", "", null);
  expect(result.sha).toBe("abc123def");
  expect(result.set).toEqual(["abc123def"]);
});

test("merge card: active set is the member list", () => {
  const result = resolveActiveSet("merge1234", "aaa111aaa bbb222bbb", null);
  expect(result.sha).toBe("merge1234");
  expect(result.set).toEqual(["aaa111aaa", "bbb222bbb"]);
});

test("toggling the same sha clears focus", () => {
  const result = resolveActiveSet("abc123def", "", "abc123def");
  expect(result.sha).toBeNull();
  expect(result.set).toBeNull();
});
