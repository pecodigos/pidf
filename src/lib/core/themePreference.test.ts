import { describe, expect, it, vi } from "vitest";

import {
  applyThemePreference,
  readStoredThemePreference,
  persistThemePreference,
  type StorageLike,
  type MatchMediaLike,
} from "./themePreference";

function createStorage(initial: Record<string, string> = {}): StorageLike {
  const store = new Map(Object.entries(initial));

  return {
    getItem(key) {
      return store.has(key) ? store.get(key)! : null;
    },
    setItem(key, value) {
      store.set(key, value);
    },
  };
}

describe("theme preference", () => {
  it("reads stored dark/light values", () => {
    expect(readStoredThemePreference(createStorage({ "pidf.theme": "dark" }))).toBe(true);
    expect(readStoredThemePreference(createStorage({ "pidf.theme": "light" }))).toBe(false);
  });

  it("returns null for unknown values", () => {
    expect(readStoredThemePreference(createStorage({ "pidf.theme": "unknown" }))).toBeNull();
    expect(readStoredThemePreference(createStorage())).toBeNull();
  });

  it("persists theme preference", () => {
    const storage = createStorage();
    persistThemePreference(storage, true);
    expect(storage.getItem("pidf.theme")).toBe("dark");

    persistThemePreference(storage, false);
    expect(storage.getItem("pidf.theme")).toBe("light");
  });

  it("applies stored preference over system preference", () => {
    const storage = createStorage({ "pidf.theme": "dark" });
    const matchMedia: MatchMediaLike = vi.fn(() => ({ matches: false }));

    expect(applyThemePreference(storage, matchMedia)).toBe(true);
  });

  it("falls back to system preference when no stored preference exists", () => {
    const storage = createStorage();
    const matchMedia: MatchMediaLike = vi.fn(() => ({ matches: true }));

    expect(applyThemePreference(storage, matchMedia)).toBe(true);
  });
});
