import { writable } from "svelte/store";

export const MIN_ZOOM = 0.5;
export const MAX_ZOOM = 3;
export const ZOOM_STEP = 0.1;

export const zoom = writable(1);
export const currentPage = writable(1);
export const pageCount = writable(0);
export const fileName = writable("No PDF loaded");
export const darkMode = writable(true);

export function clampZoom(value: number): number {
  return Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, Number(value) || 1));
}

export function updateZoom(nextZoom: number): void {
  zoom.set(clampZoom(nextZoom));
}

export function zoomIn(): void {
  zoom.update((value) => clampZoom(value + ZOOM_STEP));
}

export function zoomOut(): void {
  zoom.update((value) => clampZoom(value - ZOOM_STEP));
}

export function resetZoom(): void {
  zoom.set(1);
}

export function setCurrentPage(nextPage: number): void {
  currentPage.set(Math.max(1, Math.floor(nextPage)));
}

export function setPageCount(totalPages: number): void {
  const normalizedTotal = Math.max(0, Math.floor(totalPages));
  pageCount.set(normalizedTotal);

  currentPage.update((page) => {
    if (normalizedTotal === 0) {
      return 1;
    }

    return Math.max(1, Math.min(page, normalizedTotal));
  });
}

export function resetViewerState(): void {
  setPageCount(0);
  setCurrentPage(1);
  resetZoom();
}
