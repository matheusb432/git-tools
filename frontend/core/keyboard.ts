export type KeyboardCommand = "fold-all" | "focus-filter" | "next-file" | "previous-file" | "blur-input" | "none";

export type KeyboardEventLike = {
  readonly key: string;
  readonly code: string;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
  readonly target: EventTarget | null;
};

export function keyboardCommand(event: KeyboardEventLike): KeyboardCommand {
  if (isFormField(event.target)) return event.key === "Escape" ? "blur-input" : "none";
  if (event.altKey && event.shiftKey && event.code === "KeyC") return "fold-all";
  switch (event.key) {
    case "/":
      return "focus-filter";
    case "j":
      return "next-file";
    case "k":
      return "previous-file";
    default:
      return "none";
  }
}

// HTML elements report an uppercase tagName; comparing directly avoids allocating
// a lowercase copy on every document keydown.
function isFormField(target: EventTarget | null): boolean {
  if (target === null || !("tagName" in target)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}
