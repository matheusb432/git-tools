import { applyStoredTheme } from "./theme-boot";

try {
  applyStoredTheme();
} catch {
  // No usable storage (hardened file:// contexts): the server default stands.
}
