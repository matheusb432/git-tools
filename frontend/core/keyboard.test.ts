import { expect, test } from "bun:test";
import { keyboardCommand, type KeyboardEventLike } from "./keyboard";

function event(overrides: Partial<KeyboardEventLike>): KeyboardEventLike {
  return { key: "", code: "", altKey: false, shiftKey: false, target: null, ...overrides };
}

test("keyboardCommand maps navigation keys outside inputs", () => {
  expect(keyboardCommand(event({ key: "/" }))).toBe("focus-filter");
  expect(keyboardCommand(event({ key: "j" }))).toBe("next-file");
  expect(keyboardCommand(event({ key: "k" }))).toBe("previous-file");
});

test("keyboardCommand maps alt shift c to fold-all", () => {
  expect(keyboardCommand(event({ altKey: true, shiftKey: true, code: "KeyC" }))).toBe("fold-all");
});
