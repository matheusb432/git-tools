export const THEMES = ["dark", "light", "hearth"] as const;
export type Theme = (typeof THEMES)[number];

export type StorageLike = Pick<Storage, "getItem" | "setItem">;

const STORAGE_KEY = "gtl-theme";

export function toTheme(raw: string | null): Theme {
  return (THEMES as readonly string[]).includes(raw ?? "") ? (raw as Theme) : "dark";
}

export function readStoredTheme(storage: StorageLike): Theme | undefined {
  try {
    const raw = storage.getItem(STORAGE_KEY);
    return raw === null ? undefined : toTheme(raw);
  } catch (_error) {
    return undefined;
  }
}

export function writeStoredTheme(storage: StorageLike, theme: Theme): void {
  try {
    storage.setItem(STORAGE_KEY, theme);
  } catch (_error) {
    return;
  }
}
