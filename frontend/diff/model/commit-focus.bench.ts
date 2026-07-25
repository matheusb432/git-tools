import { bench } from "vitest";
import { resolveActiveSet } from "./commit-focus";

// Models commit-card clicks on a shelf of merged cards: each click re-resolves
// the active selection from the card's space-joined member list.
const MEMBERS = Array.from({ length: 24 }, (_, index) => `sha${index.toString(16).padStart(7, "0")}`).join(" ");

bench("resolveActiveSet select and toggle", () => {
  resolveActiveSet("sha0000003", MEMBERS, null);
  resolveActiveSet("sha0000003", MEMBERS, "sha0000003");
  resolveActiveSet("sha0000011", "", "sha0000003");
});
