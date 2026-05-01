const THEME_STORAGE_KEY = "pidf.theme";

export interface StorageLike {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export type MatchMediaLike = (query: string) => { matches: boolean };

export function readStoredThemePreference(storage: StorageLike): boolean | null {
  try {
    const storedValue = storage.getItem(THEME_STORAGE_KEY);
    if (storedValue === "dark") {
      return true;
    }

    if (storedValue === "light") {
      return false;
    }
  } catch {
    // best-effort read
  }

  return null;
}

export function persistThemePreference(storage: StorageLike, nextIsDark: boolean): void {
  try {
    storage.setItem(THEME_STORAGE_KEY, nextIsDark ? "dark" : "light");
  } catch {
    // best-effort write
  }
}

export function applyThemePreference(
  storage: StorageLike,
  matchMedia: MatchMediaLike,
): boolean {
  const storedPreference = readStoredThemePreference(storage);
  if (storedPreference !== null) {
    return storedPreference;
  }

  return matchMedia("(prefers-color-scheme: dark)").matches;
}
