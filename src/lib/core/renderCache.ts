export interface CachedRender {
  imageUrl: string;
  cssWidth: number;
  cssHeight: number;
}

export class RenderCache {
  #entries = new Map<string, CachedRender>();
  #maxEntries: number;

  constructor(maxEntries = 20) {
    this.#maxEntries = Math.max(4, maxEntries);
  }

  get(key: string): CachedRender | undefined {
    const entry = this.#entries.get(key);
    if (!entry) {
      return undefined;
    }

    this.#entries.delete(key);
    this.#entries.set(key, entry);
    return entry;
  }

  set(key: string, value: CachedRender): void {
    const existing = this.#entries.get(key);
    if (existing) {
      this.#entries.delete(key);
    }

    this.#entries.set(key, value);

    while (this.#entries.size > this.#maxEntries) {
      const oldestKey = this.#entries.keys().next().value;
      if (!oldestKey) {
        break;
      }

      this.#entries.delete(oldestKey);
    }
  }

  clear(): void {
    this.#entries.clear();
  }
}

export function cacheKey(pageNumber: number, targetWidth: number): string {
  return `${pageNumber}:${Math.round(targetWidth)}`;
}
