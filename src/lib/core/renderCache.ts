export interface CachedRender {
  imageUrl: string;
  cssWidth: number;
  cssHeight: number;
}

export class RenderCache {
  #entries = new Map<string, CachedRender>();
  #maxEntries: number;
  #onEvict?: (entry: CachedRender) => void;

  constructor(maxEntries = 20, onEvict?: (entry: CachedRender) => void) {
    this.#maxEntries = Math.max(4, maxEntries);
    this.#onEvict = onEvict;
  }

  get(key: string): CachedRender | undefined {
    const entry = this.#entries.get(key);
    if (!entry) {
      return undefined;
    }

    // LRU: re-insert at end
    this.#entries.delete(key);
    this.#entries.set(key, entry);
    return entry;
  }

  set(key: string, value: CachedRender): void {
    const existing = this.#entries.get(key);
    if (existing) {
      this.#entries.delete(key);
      this.#evictEntry(existing);
    }

    this.#entries.set(key, value);

    while (this.#entries.size > this.#maxEntries) {
      const oldestKey = this.#entries.keys().next().value;
      if (!oldestKey) {
        break;
      }
      const oldest = this.#entries.get(oldestKey);
      if (oldest) {
        this.#entries.delete(oldestKey);
        this.#evictEntry(oldest);
      }
    }
  }

  clear(): void {
    for (const entry of this.#entries.values()) {
      this.#evictEntry(entry);
    }
    this.#entries.clear();
  }

  #evictEntry(entry: CachedRender): void {
    if (this.#onEvict) {
      try {
        this.#onEvict(entry);
      } catch {
        // eviction callback is best-effort
      }
    }
  }
}

export function cacheKey(pageNumber: number, targetWidth: number): string {
  return `${pageNumber}:${Math.round(targetWidth)}`;
}
