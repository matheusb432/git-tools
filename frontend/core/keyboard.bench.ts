import "../test/dom-stub";
import { bench } from "vitest";
import { keyboardCommand, type KeyboardEventLike } from "./keyboard";

// Models the document-level keydown stream: mostly plain typing that must bail
// fast, with the occasional bound command and in-input Escape.
const input = document.createElement("input");
const EVENTS: readonly KeyboardEventLike[] = [
  { key: "a", code: "KeyA", altKey: false, shiftKey: false, target: document.body },
  { key: "Shift", code: "ShiftLeft", altKey: false, shiftKey: true, target: document.body },
  { key: "/", code: "Slash", altKey: false, shiftKey: false, target: document.body },
  { key: "j", code: "KeyJ", altKey: false, shiftKey: false, target: document.body },
  { key: "k", code: "KeyK", altKey: false, shiftKey: false, target: document.body },
  { key: "C", code: "KeyC", altKey: true, shiftKey: true, target: document.body },
  { key: "x", code: "KeyX", altKey: false, shiftKey: false, target: input },
  { key: "Escape", code: "Escape", altKey: false, shiftKey: false, target: input },
];

bench("keyboardCommand over a mixed keydown batch", () => {
  for (const event of EVENTS) keyboardCommand(event);
});
