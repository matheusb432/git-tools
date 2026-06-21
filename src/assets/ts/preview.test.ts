import { expect, test } from "bun:test";
import { navigateToFile, resolveActiveSet, toggleLongLine } from "./preview";

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

test("navigateToFile opens the target (materializing a collapsed giant) before landing", () => {
  let opened = false;
  let landed = false;
  const target: any = { set open(v: boolean) { opened = v; }, get open() { return opened; } };
  const scroller: any = {};
  navigateToFile(target, scroller, {
    raf: (cb: any) => { cb(0); return 0; },
    land: () => { landed = opened; }, // assert open happened first
  });
  expect(opened).toBe(true);
  expect(landed).toBe(true);
});

test("toggleLongLine flips expanded + aria on the owning row", () => {
  const row: any = { classList: { _on: false, toggle(c: string) { this._on = !this._on; return this._on; } } };
  const btn: any = { closest: (sel: string) => (sel === ".dl-long" ? row : null), setAttribute(k: string, v: string) { this[k] = v; } };
  toggleLongLine(btn);
  expect(row.classList._on).toBe(true);
  expect(btn["aria-expanded"]).toBe("true");
  // second click collapses again
  toggleLongLine(btn);
  expect(row.classList._on).toBe(false);
  expect(btn["aria-expanded"]).toBe("false");
});
