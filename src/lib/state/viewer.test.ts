import { afterEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import {
  MAX_ZOOM,
  MIN_ZOOM,
  clampZoom,
  currentPage,
  pageCount,
  resetViewerState,
  setCurrentPage,
  setPageCount,
  updateZoom,
  zoom,
  zoomIn,
  zoomOut,
} from "./viewer";

afterEach(() => {
  resetViewerState();
});

describe("viewer state helpers", () => {
  it("clampZoom enforces configured bounds", () => {
    expect(clampZoom(-10)).toBe(MIN_ZOOM);
    expect(clampZoom(999)).toBe(MAX_ZOOM);
    expect(clampZoom(1.75)).toBe(1.75);
  });

  it("setPageCount keeps current page within bounds", () => {
    setPageCount(10);
    setCurrentPage(8);

    setPageCount(3);

    expect(get(pageCount)).toBe(3);
    expect(get(currentPage)).toBe(3);
  });

  it("setPageCount zero resets current page to 1", () => {
    setPageCount(10);
    setCurrentPage(5);

    setPageCount(0);

    expect(get(pageCount)).toBe(0);
    expect(get(currentPage)).toBe(1);
  });

  it("zoom helpers update zoom store safely", () => {
    updateZoom(1);
    zoomIn();
    zoomOut();

    expect(get(zoom)).toBe(1);

    updateZoom(99);
    expect(get(zoom)).toBe(MAX_ZOOM);

    updateZoom(-99);
    expect(get(zoom)).toBe(MIN_ZOOM);
  });
});
