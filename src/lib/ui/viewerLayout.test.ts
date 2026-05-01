import { describe, expect, it } from "vitest";

import {
  buildPageOffsets,
  calculateAdaptivePrefetchWindow,
  calculateTargetPageWidth,
  estimatePageHeight,
  getPageStartOffset,
  resolvePageFromOffset,
} from "./viewerLayout";

describe("viewerLayout", () => {
  it("calculates snapped target width within bounds", () => {
    const width = calculateTargetPageWidth({
      containerWidth: 1200,
      containerHeight: 900,
      zoomLevel: 1,
      ratio: Math.SQRT2,
      minRenderWidth: 240,
      maxRenderWidth: 1280,
      pageFitPadding: 32,
      targetWidthStep: 48,
      fallbackWidth: 960,
      fallbackHeight: 720,
    });

    expect(width % 48).toBe(0);
    expect(width).toBeGreaterThanOrEqual(240);
    expect(width).toBeLessThanOrEqual(1280);
  });

  it("builds offsets and total height deterministically", () => {
    const result = buildPageOffsets({
      pageCount: 3,
      pageRatios: [1.2, 1.5, 1.0],
      pageTargetWidth: 600,
      defaultRatio: Math.SQRT2,
      pageGap: 12,
    });

    expect(result.pageStartOffsets.length).toBe(5);
    expect(result.pageStartOffsets[1]).toBe(0);
    expect(result.totalContentHeight).toBeGreaterThan(result.pageStartOffsets[3] ?? 0);
  });

  it("resolves page number from scroll offset", () => {
    const offsets = [0, 0, 300, 700, 1200];

    expect(
      resolvePageFromOffset({
        targetOffset: 0,
        pageCount: 3,
        totalContentHeight: 1200,
        pageStartOffsets: offsets,
      }),
    ).toBe(1);

    expect(
      resolvePageFromOffset({
        targetOffset: 350,
        pageCount: 3,
        totalContentHeight: 1200,
        pageStartOffsets: offsets,
      }),
    ).toBe(2);

    expect(
      resolvePageFromOffset({
        targetOffset: 1100,
        pageCount: 3,
        totalContentHeight: 1200,
        pageStartOffsets: offsets,
      }),
    ).toBe(3);
  });

  it("computes page start offset and fallback height", () => {
    expect(getPageStartOffset([0], 0, 1)).toBe(0);
    expect(getPageStartOffset([0, 0, 300], 1, 2)).toBe(300);
    expect(estimatePageHeight([], 1, 100, 1.4)).toBe(140);
  });

  it("computes adaptive prefetch window based on velocity and direction", () => {
    expect(
      calculateAdaptivePrefetchWindow({
        velocityPxPerMs: 0,
        direction: 1,
        minPages: 4,
        maxPages: 32,
        basePages: 10,
        velocityScale: 8,
      }),
    ).toEqual({ dynamicWindow: 10, beforePages: 6, afterPages: 12 });

    expect(
      calculateAdaptivePrefetchWindow({
        velocityPxPerMs: 2,
        direction: 1,
        minPages: 4,
        maxPages: 32,
        basePages: 10,
        velocityScale: 8,
      }),
    ).toEqual({ dynamicWindow: 26, beforePages: 15, afterPages: 31 });

    expect(
      calculateAdaptivePrefetchWindow({
        velocityPxPerMs: 2,
        direction: -1,
        minPages: 4,
        maxPages: 32,
        basePages: 10,
        velocityScale: 8,
      }),
    ).toEqual({ dynamicWindow: 26, beforePages: 31, afterPages: 15 });
  });
});
