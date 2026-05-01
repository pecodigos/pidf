export interface TargetPageWidthOptions {
  containerWidth: number;
  containerHeight: number;
  zoomLevel: number;
  ratio: number;
  minRenderWidth: number;
  maxRenderWidth: number;
  pageFitPadding: number;
  targetWidthStep: number;
  fallbackWidth: number;
  fallbackHeight: number;
}

export interface PageOffsetsOptions {
  pageCount: number;
  pageRatios: number[];
  pageTargetWidth: number;
  defaultRatio: number;
  pageGap: number;
  containerHeight?: number;
}

export interface PageOffsetsResult {
  pageStartOffsets: number[];
  totalContentHeight: number;
}

export interface ResolvePageFromOffsetOptions {
  targetOffset: number;
  pageCount: number;
  totalContentHeight: number;
  pageStartOffsets: number[];
}

export interface AdaptivePrefetchWindowOptions {
  velocityPxPerMs: number;
  direction: 1 | -1;
  minPages: number;
  maxPages: number;
  basePages: number;
  velocityScale: number;
}

export interface AdaptivePrefetchWindow {
  dynamicWindow: number;
  beforePages: number;
  afterPages: number;
}

export function calculateAdaptivePrefetchWindow(
  options: AdaptivePrefetchWindowOptions,
): AdaptivePrefetchWindow {
  const normalizedMin = Math.max(1, Math.floor(options.minPages));
  const normalizedMax = Math.max(normalizedMin, Math.floor(options.maxPages));
  const normalizedBase = Math.max(normalizedMin, Math.floor(options.basePages));
  const velocity = Math.max(0, Number.isFinite(options.velocityPxPerMs) ? options.velocityPxPerMs : 0);
  const scale = Math.max(0, Number.isFinite(options.velocityScale) ? options.velocityScale : 0);

  const dynamicWindow = Math.min(
    normalizedMax,
    Math.max(normalizedMin, Math.round(normalizedBase + velocity * scale)),
  );

  if (options.direction > 0) {
    return {
      dynamicWindow,
      beforePages: Math.max(1, Math.floor(dynamicWindow * 0.6)),
      afterPages: Math.max(1, Math.floor(dynamicWindow * 1.2)),
    };
  }

  return {
    dynamicWindow,
    beforePages: Math.max(1, Math.floor(dynamicWindow * 1.2)),
    afterPages: Math.max(1, Math.floor(dynamicWindow * 0.6)),
  };
}

export function calculateTargetPageWidth(options: TargetPageWidthOptions): number {
  const measuredWidth = options.containerWidth > 0 ? options.containerWidth : options.fallbackWidth;
  const measuredHeight =
    options.containerHeight > 0 ? options.containerHeight : options.fallbackHeight;
  const availableWidth = Math.max(options.minRenderWidth, measuredWidth - 48);
  const availableHeight = Math.max(180, measuredHeight - options.pageFitPadding);
  const safeRatio = options.ratio > 0 ? options.ratio : Math.SQRT2;

  const fitHeightWidth = availableHeight / safeRatio;
  const baseWidth = Math.max(options.minRenderWidth, Math.min(availableWidth, fitHeightWidth));
  const rawTargetWidth = Math.min(options.maxRenderWidth, baseWidth * options.zoomLevel);
  const snappedTargetWidth = Math.round(rawTargetWidth / options.targetWidthStep) * options.targetWidthStep;

  return Math.max(options.minRenderWidth, Math.min(options.maxRenderWidth, snappedTargetWidth));
}

export function estimatePageHeight(
  pageRatios: number[],
  pageNumber: number,
  width: number,
  defaultRatio: number,
): number {
  const ratio = pageRatios[pageNumber - 1] || defaultRatio;
  return Math.max(120, width * ratio);
}

export function buildPageOffsets(options: PageOffsetsOptions): PageOffsetsResult {
  if (options.pageCount <= 0) {
    return {
      pageStartOffsets: [0],
      totalContentHeight: 0,
    };
  }

  const pageStartOffsets = new Array(options.pageCount + 2).fill(0);
  let accumulated = 0;

  for (let page = 1; page <= options.pageCount; page += 1) {
    pageStartOffsets[page] = accumulated;
    const rawHeight = estimatePageHeight(
        options.pageRatios,
        page,
        options.pageTargetWidth,
        options.defaultRatio,
      );
    const slotHeight = options.containerHeight ? Math.max(options.containerHeight, rawHeight) : rawHeight;
    accumulated += slotHeight + options.pageGap;
  }

  pageStartOffsets[options.pageCount + 1] = accumulated;

  return {
    pageStartOffsets,
    totalContentHeight: accumulated,
  };
}

export function getPageStartOffset(
  pageStartOffsets: number[],
  pageCount: number,
  pageNumber: number,
): number {
  if (pageCount <= 0) {
    return 0;
  }

  const normalizedPage = Math.max(1, Math.min(pageCount + 1, Math.floor(pageNumber)));
  return pageStartOffsets[normalizedPage] ?? 0;
}

export function resolvePageFromOffset(options: ResolvePageFromOffsetOptions): number {
  if (options.pageCount <= 0) {
    return 1;
  }

  const safeOffset = Math.max(
    0,
    Math.min(options.targetOffset, Math.max(0, options.totalContentHeight - 1)),
  );
  let low = 1;
  let high = options.pageCount;

  while (low < high) {
    const mid = Math.floor((low + high + 1) / 2);
    const startOffset = options.pageStartOffsets[mid] ?? 0;

    if (startOffset <= safeOffset) {
      low = mid;
    } else {
      high = mid - 1;
    }
  }

  return low;
}
