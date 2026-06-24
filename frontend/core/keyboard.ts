export type KeyboardCommand = "fold-all" | "focus-filter" | "next-file" | "previous-file" | "blur-input" | "none";

export type KeyboardEventLike = {
  readonly key: string;
  readonly code: string;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
  readonly target: EventTarget | null;
};

export function keyboardCommand(event: KeyboardEventLike): KeyboardCommand {
  const t = event.target;
  const tag =
    t !== null &&
    typeof t === "object" &&
    "tagName" in t &&
    typeof (t as Record<string, unknown>)["tagName"] === "string"
      ? ((t as Record<string, unknown>)["tagName"] as string).toLowerCase()
      : "";
  if (tag === "input" || tag === "textarea" || tag === "select") {
    return event.key === "Escape" ? "blur-input" : "none";
  }
  if (event.altKey && event.shiftKey && event.code === "KeyC") return "fold-all";
  if (event.key === "/") return "focus-filter";
  if (event.key === "j") return "next-file";
  if (event.key === "k") return "previous-file";
  return "none";
}
