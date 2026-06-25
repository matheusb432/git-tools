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

test("keyboardCommand routes Escape inside an input to blur-input", () => {
  expect(keyboardCommand(event({ key: "Escape", target: { tagName: "INPUT" } as unknown as EventTarget }))).toBe("blur-input");
});

test("keyboardCommand ignores navigation keys typed inside an input", () => {
  expect(keyboardCommand(event({ key: "j", target: { tagName: "TEXTAREA" } as unknown as EventTarget }))).toBe("none");
});
