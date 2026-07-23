import { applyStoredTheme } from "./theme-boot";

try {
  applyStoredTheme(localStorage, document.documentElement);
} catch {
  // No usable storage (hardened file:// contexts): the server default stands.
}
