const THEME_STORAGE_KEY = "gtl-theme";

// Runs inline in <head> before first paint so the stored per-device theme wins
// over the server-rendered default without a flash. It must touch nothing but
// the theme: layout and density preferences realize after paint.
export function applyStoredTheme(): void {
  const theme = localStorage.getItem(THEME_STORAGE_KEY);
  if (theme) document.documentElement.dataset["theme"] = theme;
}
