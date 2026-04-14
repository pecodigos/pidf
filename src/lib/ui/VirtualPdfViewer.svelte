<script lang="ts">
  import { browser } from "$app/environment";
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";

  import type { PdfSession } from "$lib/core/pdf";
  import { RenderCache } from "$lib/core/renderCache";
  import { logPdfStage } from "$lib/core/trace";
  import { clampZoom, ZOOM_STEP } from "$lib/state/viewer";
  import PageCanvas from "$lib/ui/PageCanvas.svelte";

  export let session: PdfSession | null = null;
  export let zoom = 1;

  const dispatch = createEventDispatcher<{
    pagechange: { page: number };
    zoomchange: { zoom: number };
    firstrender: { attemptId: string; page: number };
    rendererror: { attemptId: string; page: number; message: string };
  }>();

  const PAGE_GAP = 12;
  const DEFAULT_RATIO = Math.SQRT2;
  const ACTIVE_PAGES_BEFORE = 1;
  const ACTIVE_PAGES_AFTER = 2;
  const RENDER_PAGES_BEFORE = 10;
  const RENDER_PAGES_AFTER = 14;
  const RENDER_WINDOW_SHIFT_MARGIN = 4;
  const RESIZE_COMMIT_MS = 180;
  const TARGET_WIDTH_STEP = 48;
  const MIN_RENDER_WIDTH = 240;
  const PAGE_FIT_PADDING = 32;
  const MAX_RENDER_WIDTH = 1280;
  const ENABLE_VIEWER_DIAGNOSTICS = false;

  let container: HTMLDivElement;
  let pageCount = 0;
  let renderedPageNumbers: number[] = [];
  let pageRatios: number[] = [];
  let pageStartOffsets: number[] = [0];
  let renderWindowStart = 1;
  let renderWindowEnd = 0;
  let totalContentHeight = 0;
  let topSpacerHeight = 0;
  let bottomSpacerHeight = 0;
  let activeMap: Record<number, boolean> = {};
  let resizeObserver: ResizeObserver | null = null;
  let scrollFrame: number | null = null;
  let resizeDebounce: ReturnType<typeof setTimeout> | null = null;
  let pendingContainerWidth = 0;
  let zoomWheelTimer: ReturnType<typeof setTimeout> | null = null;
  let zoomWheelDelta = 0;
  let currentPage = 1;
  let containerWidth = 0;
  let containerHeight = 0;
  let pendingContainerHeight = 0;
  let pageTargetWidth = 900;
  let lastSession: PdfSession | null = null;
  let firstRenderCommitted = false;
  let lastLoggedActivePage = 0;
  let lastGeometryWidth = 0;
  let geometryFrame: number | null = null;
  let pendingGeometryPreviousWidth = 0;
  let pendingGeometryNextWidth = 0;

  const renderCache = new RenderCache(24);

  function calculateTargetPageWidth(
    width: number,
    height: number,
    zoomLevel: number,
    ratio: number,
  ): number {
    const measuredWidth = width > 0 ? width : browser ? window.innerWidth : 960;
    const measuredHeight = height > 0 ? height : browser ? window.innerHeight : 720;
    const availableWidth = Math.max(MIN_RENDER_WIDTH, measuredWidth - 48);
    const availableHeight = Math.max(180, measuredHeight - PAGE_FIT_PADDING);
    const safeRatio = ratio > 0 ? ratio : DEFAULT_RATIO;

    // At 100%, prefer a fit-page-height baseline similar to browser PDF readers.
    const fitHeightWidth = availableHeight / safeRatio;
    const baseWidth = Math.max(MIN_RENDER_WIDTH, Math.min(availableWidth, fitHeightWidth));
    const rawTargetWidth = Math.min(MAX_RENDER_WIDTH, baseWidth * zoomLevel);
    const snappedTargetWidth =
      Math.round(rawTargetWidth / TARGET_WIDTH_STEP) * TARGET_WIDTH_STEP;
    return Math.max(MIN_RENDER_WIDTH, Math.min(MAX_RENDER_WIDTH, snappedTargetWidth));
  }

  function estimatedPageHeight(pageNumber: number, width = pageTargetWidth): number {
    const ratio = pageRatios[pageNumber - 1] || DEFAULT_RATIO;
    return Math.max(120, width * ratio);
  }

  function pageBlockHeight(pageNumber: number): number {
    return estimatedPageHeight(pageNumber, pageTargetWidth) + PAGE_GAP;
  }

  function rebuildPageOffsets(): void {
    if (pageCount <= 0) {
      pageStartOffsets = [0];
      totalContentHeight = 0;
      return;
    }

    const nextOffsets = new Array(pageCount + 2).fill(0);
    let accumulated = 0;

    for (let page = 1; page <= pageCount; page += 1) {
      nextOffsets[page] = accumulated;
      accumulated += pageBlockHeight(page);
    }

    nextOffsets[pageCount + 1] = accumulated;
    pageStartOffsets = nextOffsets;
    totalContentHeight = accumulated;
  }

  function pageStartOffset(pageNumber: number): number {
    if (pageCount <= 0) {
      return 0;
    }

    const normalized = Math.max(1, Math.min(pageCount + 1, Math.floor(pageNumber)));
    return pageStartOffsets[normalized] ?? 0;
  }

  function updateRenderedWindow(centerPage: number): void {
    if (pageCount <= 0) {
      renderWindowStart = 1;
      renderWindowEnd = 0;
      renderedPageNumbers = [];
      topSpacerHeight = 0;
      bottomSpacerHeight = 0;
      return;
    }

    const desiredStart = Math.max(1, centerPage - RENDER_PAGES_BEFORE);
    const desiredEnd = Math.min(pageCount, centerPage + RENDER_PAGES_AFTER);

    const shouldShiftWindow =
      renderWindowEnd < renderWindowStart ||
      centerPage <= renderWindowStart + RENDER_WINDOW_SHIFT_MARGIN ||
      centerPage >= renderWindowEnd - RENDER_WINDOW_SHIFT_MARGIN;

    if (shouldShiftWindow) {
      renderWindowStart = desiredStart;
      renderWindowEnd = desiredEnd;
    } else {
      return;
    }

    const renderStart = renderWindowStart;
    const renderEnd = renderWindowEnd;

    topSpacerHeight = pageStartOffset(renderStart);
    bottomSpacerHeight = Math.max(0, totalContentHeight - pageStartOffset(renderEnd + 1));
    renderedPageNumbers = Array.from(
      { length: renderEnd - renderStart + 1 },
      (_, index) => renderStart + index,
    );
  }

  function handleGeometryChange(previousWidth: number): void {
    if (pageCount <= 0) {
      return;
    }

    let anchorPage = currentPage;
    let anchorRatio = 0;

    if (container) {
      const viewportHeight = Math.max(1, container.clientHeight);
      const markerOffset = container.scrollTop + Math.max(80, viewportHeight * 0.2);
      anchorPage = pageFromOffset(markerOffset);

      const previousPageStart = pageStartOffset(anchorPage);
      const previousPageBlockHeight = Math.max(
        1,
        estimatedPageHeight(anchorPage, previousWidth) + PAGE_GAP,
      );
      const offsetWithinPage = container.scrollTop - previousPageStart;
      anchorRatio = Math.max(0, Math.min(1, offsetWithinPage / previousPageBlockHeight));
    }

    rebuildPageOffsets();
    renderWindowStart = 1;
    renderWindowEnd = 0;

    if (container) {
      const newPageStart = pageStartOffset(anchorPage);
      const newPageBlockHeight = Math.max(1, pageBlockHeight(anchorPage));
      const nextScrollTop = newPageStart + anchorRatio * newPageBlockHeight;
      const maxScrollTop = Math.max(0, totalContentHeight - container.clientHeight);
      container.scrollTop = Math.max(0, Math.min(maxScrollTop, nextScrollTop));
    }

    updateRenderedWindow(anchorPage);
    updateActiveFromScroll();
  }

  function scheduleGeometryChange(previousWidth: number, nextWidth: number): void {
    if (!browser) {
      handleGeometryChange(previousWidth);
      lastGeometryWidth = nextWidth;
      return;
    }

    if (pendingGeometryPreviousWidth === 0) {
      pendingGeometryPreviousWidth = previousWidth;
    }

    pendingGeometryNextWidth = nextWidth;

    if (geometryFrame !== null) {
      return;
    }

    geometryFrame = window.requestAnimationFrame(() => {
      geometryFrame = null;

      const widthBefore = pendingGeometryPreviousWidth > 0
        ? pendingGeometryPreviousWidth
        : previousWidth;
      const widthAfter = pendingGeometryNextWidth > 0 ? pendingGeometryNextWidth : nextWidth;

      pendingGeometryPreviousWidth = 0;
      pendingGeometryNextWidth = 0;

      handleGeometryChange(widthBefore);
      lastGeometryWidth = widthAfter;
    });
  }

  function emitCurrentPage(nextPage: number): void {
    const normalized = Math.max(1, Math.min(pageCount || 1, Math.floor(nextPage)));
    if (normalized === currentPage) {
      return;
    }

    currentPage = normalized;
    dispatch("pagechange", { page: currentPage });
  }

  function setActivePages(pages: number[]): void {
    const nextMap: Record<number, boolean> = {};

    for (const page of pages) {
      if (page >= 1 && page <= pageCount) {
        nextMap[page] = true;
      }
    }

    // Always keep the first page warm to avoid empty first paint.
    if (pageCount > 0) {
      nextMap[1] = true;
    }

    const currentKeys = Object.keys(activeMap);
    const nextKeys = Object.keys(nextMap);

    if (currentKeys.length === nextKeys.length) {
      let unchanged = true;

      for (const key of nextKeys) {
        if (!Object.prototype.hasOwnProperty.call(activeMap, key)) {
          unchanged = false;
          break;
        }
      }

      if (unchanged) {
        return;
      }
    }

    activeMap = nextMap;
  }

  function pageFromOffset(targetOffset: number): number {
    if (pageCount === 0) {
      return 1;
    }

    const safeOffset = Math.max(0, Math.min(targetOffset, Math.max(0, totalContentHeight - 1)));
    let low = 1;
    let high = pageCount;

    while (low < high) {
      const mid = Math.floor((low + high + 1) / 2);
      const startOffset = pageStartOffsets[mid] ?? 0;

      if (startOffset <= safeOffset) {
        low = mid;
      } else {
        high = mid - 1;
      }
    }

    return low;
  }

  function setActiveWindow(centerPage: number): void {
    const pages: number[] = [];
    for (
      let page = centerPage - ACTIVE_PAGES_BEFORE;
      page <= centerPage + ACTIVE_PAGES_AFTER;
      page += 1
    ) {
      pages.push(page);
    }

    setActivePages(pages);
    updateRenderedWindow(centerPage);
  }

  function updateActiveFromScroll(): void {
    if (!container || pageCount === 0) {
      return;
    }

    const previousPage = currentPage;
    const boundedViewportHeight = browser
      ? Math.max(1, Math.min(container.clientHeight, window.innerHeight || container.clientHeight))
      : Math.max(1, container.clientHeight);
    const markerOffset = container.scrollTop + Math.max(80, boundedViewportHeight * 0.2);
    const nextPage = pageFromOffset(markerOffset);

    if (nextPage !== previousPage) {
      setActiveWindow(nextPage);
      emitCurrentPage(nextPage);
    }

    if (nextPage !== lastLoggedActivePage) {
      lastLoggedActivePage = nextPage;

      if (ENABLE_VIEWER_DIAGNOSTICS && session) {
        const activePages = Object.keys(activeMap)
          .map((value) => Number.parseInt(value, 10))
          .filter((value) => Number.isFinite(value))
          .sort((a, b) => a - b);

        logPdfStage(
          "ui_active_window",
          {
            openAttemptId: session.diagnostics.openAttemptId,
            currentPage: nextPage,
            activeStart: activePages[0] ?? null,
            activeEnd: activePages[activePages.length - 1] ?? null,
            scrollTop: Math.round(container.scrollTop),
          },
          Date.now() - session.diagnostics.openStartedAtMs,
        );
      }
    }

    if (ENABLE_VIEWER_DIAGNOSTICS) {
      console.debug("[PiDF] active render window updated", {
        currentPage: nextPage,
        activePages: Object.keys(activeMap),
        scrollTop: container.scrollTop,
        targetWidth: pageTargetWidth,
      });
    }
  }

  function scheduleActiveWindowUpdate(): void {
    if (scrollFrame !== null) {
      return;
    }

    scrollFrame = window.requestAnimationFrame(() => {
      scrollFrame = null;
      updateActiveFromScroll();
    });
  }

  function updateActiveFromScrollFallback(): void {
    updateActiveFromScroll();
  }

  function handlePageRenderCommitted(event: CustomEvent<{ pageNumber: number }>): void {
    if (!session || firstRenderCommitted || event.detail.pageNumber !== 1) {
      return;
    }

    firstRenderCommitted = true;

    logPdfStage(
      "first_render_committed",
      {
        openAttemptId: session.diagnostics.openAttemptId,
        pageNumber: event.detail.pageNumber,
      },
      Date.now() - session.diagnostics.openStartedAtMs,
    );

    dispatch("firstrender", {
      attemptId: session.diagnostics.openAttemptId,
      page: event.detail.pageNumber,
    });
  }

  function handlePageRenderError(
    event: CustomEvent<{ pageNumber: number; message: string }>,
  ): void {
    if (!session || event.detail.pageNumber !== 1) {
      return;
    }

    logPdfStage(
      "first_render_failed",
      {
        openAttemptId: session.diagnostics.openAttemptId,
        pageNumber: event.detail.pageNumber,
        message: event.detail.message,
      },
      Date.now() - session.diagnostics.openStartedAtMs,
    );

    dispatch("rendererror", {
      attemptId: session.diagnostics.openAttemptId,
      page: event.detail.pageNumber,
      message: event.detail.message,
    });
  }

  function handleWheel(event: WheelEvent): void {
    if (!event.ctrlKey) {
      return;
    }

    event.preventDefault();
    zoomWheelDelta += event.deltaY;

    if (zoomWheelTimer) {
      return;
    }

    zoomWheelTimer = setTimeout(() => {
      const delta = zoomWheelDelta < 0 ? ZOOM_STEP : -ZOOM_STEP;
      zoomWheelDelta = 0;
      zoomWheelTimer = null;
      dispatch("zoomchange", { zoom: clampZoom(zoom + delta) });
    }, 110);
  }

  function handleKeydown(event: KeyboardEvent): void {
    const target = event.target as HTMLElement | null;
    const editing =
      target?.tagName === "INPUT" ||
      target?.tagName === "TEXTAREA" ||
      target?.isContentEditable;

    if (editing || !container) {
      return;
    }

    if (event.ctrlKey && event.key === "0") {
      event.preventDefault();
      dispatch("zoomchange", { zoom: 1 });
      return;
    }

    if (event.key === "+" || (event.key === "=" && event.ctrlKey)) {
      event.preventDefault();
      dispatch("zoomchange", { zoom: clampZoom(zoom + ZOOM_STEP) });
      return;
    }

    if (event.key === "-") {
      event.preventDefault();
      dispatch("zoomchange", { zoom: clampZoom(zoom - ZOOM_STEP) });
      return;
    }

    if (event.key === "PageDown") {
      event.preventDefault();
      container.scrollTop += container.clientHeight * 0.92;
      updateActiveFromScrollFallback();
      return;
    }

    if (event.key === "PageUp") {
      event.preventDefault();
      container.scrollTop -= container.clientHeight * 0.92;
      updateActiveFromScrollFallback();
      return;
    }

    if (event.key === "Home") {
      event.preventDefault();
      container.scrollTop = 0;
      updateActiveFromScrollFallback();
      return;
    }

    if (event.key === "End") {
      event.preventDefault();
      container.scrollTop = container.scrollHeight;
      updateActiveFromScrollFallback();
    }
  }

  function handleScroll(): void {
    scheduleActiveWindowUpdate();
  }

  function initializeSession(nextSession: PdfSession): void {
    pageCount = nextSession.pageCount;
    pageRatios = new Array(pageCount).fill(DEFAULT_RATIO);
    lastGeometryWidth = 0;
    rebuildPageOffsets();
    setActiveWindow(1);
    currentPage = 1;
    firstRenderCommitted = false;
    lastLoggedActivePage = 0;
    renderCache.clear();

    if (container) {
      container.scrollTop = 0;
      containerWidth = container.clientWidth;
    }

    dispatch("pagechange", { page: 1 });
    console.info("[PiDF] viewer session initialized", {
      pageCount,
      initialActivePages: Object.keys(activeMap),
      targetWidth: pageTargetWidth,
      diagnostics: nextSession.diagnostics,
    });

    void tick().then(() => updateActiveFromScrollFallback());

    void nextSession
      .getDefaultAspectRatio()
      .then((ratio) => {
        if (!Number.isFinite(ratio) || ratio <= 0) {
          return;
        }

        pageRatios.fill(ratio);
        pageRatios = [...pageRatios];
        rebuildPageOffsets();
        updateRenderedWindow(currentPage);
      })
      .catch(() => {
        // Keep fallback ratio.
      });
  }

  export function jumpToPage(pageNumber: number): void {
    if (!container || pageCount === 0) {
      return;
    }

    const normalizedPage = Math.max(1, Math.min(pageCount, Math.floor(pageNumber)));
    container.scrollTop = pageStartOffset(normalizedPage);

    setActivePages([
      normalizedPage - ACTIVE_PAGES_BEFORE,
      normalizedPage,
      normalizedPage + 1,
      normalizedPage + ACTIVE_PAGES_AFTER,
    ]);
    updateRenderedWindow(normalizedPage);

    emitCurrentPage(normalizedPage);
    window.requestAnimationFrame(() => updateActiveFromScroll());
  }

  onMount(() => {
    if (container) {
      containerWidth = container.clientWidth;
      containerHeight = container.clientHeight;
    }

    if (browser && typeof ResizeObserver !== "undefined" && container) {
      resizeObserver = new ResizeObserver(() => {
        if (!container) {
          return;
        }

        pendingContainerWidth = container.clientWidth;
        pendingContainerHeight = container.clientHeight;

        if (resizeDebounce) {
          clearTimeout(resizeDebounce);
        }

        resizeDebounce = setTimeout(() => {
          resizeDebounce = null;

          if (!container) {
            return;
          }

          const nextWidth = pendingContainerWidth || container.clientWidth;
          const nextHeight = pendingContainerHeight || container.clientHeight;
          if (Math.abs(nextWidth - containerWidth) >= 2) {
            containerWidth = nextWidth;
          }

          if (Math.abs(nextHeight - containerHeight) >= 2) {
            containerHeight = nextHeight;
          }

          scheduleActiveWindowUpdate();
        }, RESIZE_COMMIT_MS);
      });

      resizeObserver.observe(container);
    }

    updateActiveFromScrollFallback();
  });

  onDestroy(() => {
    if (geometryFrame !== null) {
      window.cancelAnimationFrame(geometryFrame);
      geometryFrame = null;
    }

    if (scrollFrame !== null) {
      window.cancelAnimationFrame(scrollFrame);
      scrollFrame = null;
    }

    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }

    if (zoomWheelTimer) {
      clearTimeout(zoomWheelTimer);
      zoomWheelTimer = null;
      zoomWheelDelta = 0;
    }

    if (resizeDebounce) {
      clearTimeout(resizeDebounce);
      resizeDebounce = null;
    }

    renderCache.clear();
  });

  $: if (session !== lastSession) {
    lastSession = session;

    if (!session) {
      pageCount = 0;
      renderedPageNumbers = [];
      pageRatios = [];
      pageStartOffsets = [0];
      renderWindowStart = 1;
      renderWindowEnd = 0;
      totalContentHeight = 0;
      topSpacerHeight = 0;
      bottomSpacerHeight = 0;
      activeMap = {};
      currentPage = 1;
      firstRenderCommitted = false;
      lastLoggedActivePage = 0;
      lastGeometryWidth = 0;
      renderCache.clear();
    } else {
      initializeSession(session);
    }
  }

  $: pageTargetWidth = calculateTargetPageWidth(
    containerWidth,
    containerHeight,
    zoom,
    pageRatios[0] || DEFAULT_RATIO,
  );

  $: if (pageCount > 0 && pageTargetWidth > 0 && pageTargetWidth !== lastGeometryWidth) {
    const previousWidth = lastGeometryWidth > 0 ? lastGeometryWidth : pageTargetWidth;
    scheduleGeometryChange(previousWidth, pageTargetWidth);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
<div
  class="viewer"
  bind:this={container}
  role="region"
  aria-label="PDF viewer"
  tabindex="0"
  on:scroll={handleScroll}
  on:wheel={handleWheel}
  on:keydown={handleKeydown}
>
  {#if !session}
    <div class="empty">
      <h2>Keyboard Shortcuts</h2>
      <p class="hint">Open a PDF and use these controls:</p>
      <ul class="shortcuts" aria-label="Keyboard shortcuts">
        <li><kbd>Ctrl/Cmd</kbd> + <kbd>O</kbd> Open PDF</li>
        <li><kbd>Arrow Left</kbd> / <kbd>Arrow Right</kbd> Previous/Next page</li>
        <li><kbd>Ctrl/Cmd</kbd> + <kbd>+</kbd> Zoom in</li>
        <li><kbd>Ctrl/Cmd</kbd> + <kbd>-</kbd> Zoom out</li>
        <li><kbd>Ctrl/Cmd</kbd> + <kbd>0</kbd> Reset zoom</li>
        <li><kbd>F11</kbd> or <kbd>Ctrl/Cmd</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd> Fullscreen</li>
        <li><kbd>Ctrl/Cmd</kbd> + <kbd>F</kbd> Find (page jump)</li>
      </ul>
    </div>
  {:else}
    <div class="list">
      {#if topSpacerHeight > 0}
        <div class="spacer" style={`height:${topSpacerHeight}px`} aria-hidden="true"></div>
      {/if}

      {#each renderedPageNumbers as pageNumber (pageNumber)}
        <section
          class="slot"
          style={`min-height:${estimatedPageHeight(pageNumber, pageTargetWidth) + PAGE_GAP}px`}
        >
            {#if activeMap[pageNumber]}
            <PageCanvas
              {session}
              {pageNumber}
              targetWidth={pageTargetWidth}
              cache={renderCache}
              on:rendercommitted={handlePageRenderCommitted}
              on:rendererror={handlePageRenderError}
            />
          {:else}
            <div
              class="placeholder"
              style={`height:${estimatedPageHeight(pageNumber, pageTargetWidth)}px`}
              aria-hidden="true"
            ></div>
          {/if}
        </section>
      {/each}

      {#if bottomSpacerHeight > 0}
        <div class="spacer" style={`height:${bottomSpacerHeight}px`} aria-hidden="true"></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .viewer {
    position: relative;
    overflow: auto;
    min-height: 0;
    height: 100%;
    width: 100%;
    background: var(--bg);
    outline: none;
  }

  .viewer:focus-visible {
    box-shadow: inset 0 0 0 2px color-mix(in oklab, var(--accent) 35%, transparent);
  }

  .list {
    width: 100%;
    padding-top: 0.7rem;
  }

  .spacer {
    width: 100%;
    pointer-events: none;
  }

  .slot {
    width: 100%;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    margin-bottom: 0.3rem;
  }

  .placeholder {
    width: min(1700px, calc(100% - 48px));
    max-width: calc(100% - 48px);
    border: 1px solid rgb(255 255 255 / 0.08);
    border-radius: 0.2rem;
    background: rgb(255 255 255 / 0.03);
  }

  .empty {
    min-height: 100%;
    display: grid;
    place-content: center;
    gap: 0.6rem;
    text-align: center;
    color: var(--muted);
    padding: 1.2rem;
  }

  .empty h2 {
    margin: 0;
    color: var(--text);
    font-size: 1.05rem;
    letter-spacing: 0.01em;
  }

  .empty p {
    margin: 0;
  }

  .hint {
    font-size: 0.82rem;
    max-width: 32rem;
    margin-inline: auto;
  }

  .shortcuts {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 0.38rem;
    justify-items: center;
    font-size: 0.84rem;
  }

  .shortcuts li {
    color: var(--text);
  }

  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.35rem;
    padding: 0.02rem 0.3rem;
    border: 1px solid color-mix(in oklab, var(--line) 80%, transparent);
    border-radius: 0.35rem;
    background: var(--panel-raised);
    color: var(--text);
    font: inherit;
    font-weight: 600;
  }
</style>
