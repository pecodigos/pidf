<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";

  import { withTimeout } from "$lib/core/async";
  import { cacheKey, type RenderCache } from "$lib/core/renderCache";
  import type { PdfSession } from "$lib/core/pdf";
  import { logPdfStage } from "$lib/core/trace";

  export let session: PdfSession;
  export let pageNumber: number;
  export let currentPage = 1;
  export let targetWidth = 900;
  export let cache: RenderCache;

  const dispatch = createEventDispatcher<{
    rendercommitted: { pageNumber: number };
    rendererror: { pageNumber: number; message: string };
    heightchange: { pageNumber: number; cssWidth: number; cssHeight: number };
  }>();

  const DEFAULT_RATIO = Math.SQRT2;
  const RENDER_TIMEOUT_MS = 12000;
  const INITIAL_RENDER_DEBOUNCE_MS = 40;
  const VIEWPORT_PRIORITY_DISTANCE = 1;
  const HIGH_PRIORITY_RENDER_DEBOUNCE_MS = 40;
  const LOW_PRIORITY_RENDER_DEBOUNCE_MS = 150;
  const RESIZE_RENDER_DEBOUNCE_MS = 45;
  const OFFSCREEN_RESIZE_RENDER_DEBOUNCE_MS = 180;
  const ENABLE_PAGE_DIAGNOSTICS = false;

  let imageUrl = "";
  let cssWidth = Math.max(1, targetWidth);
  let cssHeight = Math.max(120, targetWidth * DEFAULT_RATIO);
  let errorMessage = "";
  let lastRenderKey = "";
  let activeRenderId = 0;
  let renderDebounce: ReturnType<typeof setTimeout> | null = null;

  function logRenderStage(stage: string, details: Record<string, unknown> = {}): void {
    if (!ENABLE_PAGE_DIAGNOSTICS && stage !== "page_render_failed") {
      return;
    }

    logPdfStage(
      stage,
      {
        openAttemptId: session?.diagnostics.openAttemptId ?? null,
        pageNumber,
        ...details,
      },
      session ? Date.now() - session.diagnostics.openStartedAtMs : undefined,
    );
  }

  function describeError(error: unknown): { name?: string; message: string; stack?: string } {
    if (error instanceof Error) {
      return {
        name: error.name,
        message: error.message,
        stack: error.stack,
      };
    }

    return { message: String(error) };
  }

  function isRenderSupersededError(message: string): boolean {
    return message.toLowerCase().includes("superseded");
  }

  function renderPriority(distanceFromViewport: number, quality: "low" | "high"): number {
    const normalizedDistance = Math.max(0, Math.floor(distanceFromViewport));

    if (quality === "high") {
      return Math.min(4096, normalizedDistance <= 1 ? normalizedDistance : 10 + normalizedDistance * 10);
    }

    return Math.min(4096, normalizedDistance <= 1 ? 80 + normalizedDistance : 160 + normalizedDistance * 14);
  }

  async function renderPage(): Promise<void> {
    if (!session || targetWidth <= 0) {
      return;
    }

    // Quality ladder: low-res first, then high-res
    const highResWidth = Math.max(1, Math.round(targetWidth));
    const lowResWidth = Math.max(1, Math.round(targetWidth / 3));
    const highResKey = cacheKey(pageNumber, highResWidth);
    const lowResKey = cacheKey(pageNumber, lowResWidth);

    const renderId = ++activeRenderId;
    const distanceFromViewport = Math.abs(currentPage - pageNumber);
    const lowResRenderPriority = renderPriority(distanceFromViewport, "low");
    const highResRenderPriority = renderPriority(distanceFromViewport, "high");
    const isViewportPage = distanceFromViewport <= VIEWPORT_PRIORITY_DISTANCE;

    if (
      !errorMessage &&
      (highResKey === lastRenderKey ||
        (distanceFromViewport > VIEWPORT_PRIORITY_DISTANCE && lowResKey === lastRenderKey))
    ) {
      return;
    }

    errorMessage = "";

    // Try low-res first only for offscreen pages.
    let didShowLowRes = false;
    const tryShowLowRes = () => {
      if (isViewportPage) {
        return;
      }

      const cachedLow = cache.get(lowResKey);
      if (cachedLow && lastRenderKey !== highResKey) {
        imageUrl = cachedLow.imageUrl;
        cssWidth = cachedLow.cssWidth;
        if (cssHeight !== cachedLow.cssHeight) {
          cssHeight = cachedLow.cssHeight;
          dispatch("heightchange", { pageNumber, cssWidth, cssHeight });
        }
        lastRenderKey = lowResKey;
        didShowLowRes = true;
      }
    };
    tryShowLowRes();

    cssWidth = highResWidth;
    cssHeight = Math.max(120, highResWidth * DEFAULT_RATIO);

    try {
      // If high-res is cached, use it immediately
      const cachedHigh = cache.get(highResKey);
      if (cachedHigh) {
        logRenderStage("page_render_cache_hit", {
          targetWidth: highResWidth,
        });
        if (renderId !== activeRenderId) {
          return;
        }
        imageUrl = cachedHigh.imageUrl;
        cssWidth = cachedHigh.cssWidth;
        if (cssHeight !== cachedHigh.cssHeight) {
          cssHeight = cachedHigh.cssHeight;
          dispatch("heightchange", { pageNumber, cssWidth, cssHeight });
        }
        dispatch("rendercommitted", { pageNumber });
        lastRenderKey = highResKey;
        return;
      }

      if (!didShowLowRes && !isViewportPage) {
        // Only render low-res if not already showing it
        const renderedLow = await withTimeout(
          session.renderPage(pageNumber, lowResWidth, lowResRenderPriority),
          RENDER_TIMEOUT_MS,
          `Backend page render timed out after ${RENDER_TIMEOUT_MS}ms (low-res).`,
        );
        if (renderId !== activeRenderId) return;
        imageUrl = renderedLow.imageUrl;
        cssWidth = renderedLow.cssWidth;
        if (cssHeight !== renderedLow.cssHeight) {
          cssHeight = renderedLow.cssHeight;
          dispatch("heightchange", { pageNumber, cssWidth, cssHeight });
        }
        cache.set(lowResKey, {
          imageUrl: renderedLow.imageUrl,
          cssWidth: renderedLow.cssWidth,
          cssHeight: renderedLow.cssHeight,
        });
        lastRenderKey = lowResKey;
      }

      if (!isViewportPage) {
        return;
      }

      const renderedHigh = await withTimeout(
        session.renderPage(pageNumber, highResWidth, highResRenderPriority),
        RENDER_TIMEOUT_MS,
        `Backend page render timed out after ${RENDER_TIMEOUT_MS}ms (high-res).`,
      );
      if (renderId !== activeRenderId) return;
      imageUrl = renderedHigh.imageUrl;
      cssWidth = renderedHigh.cssWidth;
      if (cssHeight !== renderedHigh.cssHeight) {
        cssHeight = renderedHigh.cssHeight;
        dispatch("heightchange", { pageNumber, cssWidth, cssHeight });
      }
      cache.set(highResKey, {
        imageUrl: renderedHigh.imageUrl,
        cssWidth: renderedHigh.cssWidth,
        cssHeight: renderedHigh.cssHeight,
      });
      dispatch("rendercommitted", { pageNumber });
      lastRenderKey = highResKey;
      return;

    } catch (error) {
      if (renderId !== activeRenderId) {
        return;
      }

      const describedError = describeError(error);
      if (isRenderSupersededError(describedError.message)) {
        return;
      }

      errorMessage = `Page ${pageNumber}: ${describedError.message}`;

      dispatch("rendererror", {
        pageNumber,
        message: describedError.message,
      });

      logRenderStage("page_render_failed", {
        targetWidth: highResWidth,
        message: describedError.message,
      });

      console.error("[PiDF] page render failed", {
        pageNumber,
        targetWidth: highResWidth,
        error: describedError,
      });
    }
  }

  function scheduleRender(): void {
    if (!session || targetWidth <= 0) {
      return;
    }

    if (renderDebounce) {
      clearTimeout(renderDebounce);
      renderDebounce = null;
    }

    const distance = Math.abs(currentPage - pageNumber);
    const isHighPriority = distance <= VIEWPORT_PRIORITY_DISTANCE;
    const renderWidth = Math.max(1, Math.round(targetWidth));
    const widthChanged = Math.abs(renderWidth - Math.round(cssWidth)) >= 2;

    let debounceMs = Math.max(INITIAL_RENDER_DEBOUNCE_MS, LOW_PRIORITY_RENDER_DEBOUNCE_MS);

    if (pageNumber === 1 && !imageUrl) {
      debounceMs = 0;
    } else if (isHighPriority && widthChanged) {
      debounceMs = 0;
    } else if (isHighPriority) {
      debounceMs = HIGH_PRIORITY_RENDER_DEBOUNCE_MS;
    } else if (widthChanged) {
      debounceMs = OFFSCREEN_RESIZE_RENDER_DEBOUNCE_MS;
    } else if (imageUrl) {
      debounceMs = RESIZE_RENDER_DEBOUNCE_MS;
    }

    renderDebounce = setTimeout(() => {
      renderDebounce = null;
      void renderPage();
    }, debounceMs);
  }

  $: if (session && targetWidth > 0 && currentPage >= 0) {
    const rawTarget = Math.max(1, Math.round(targetWidth));
    if (Math.abs(cssWidth - rawTarget) >= 1 && imageUrl) {
      const currentRatio = cssHeight / Math.max(1, cssWidth);
      cssWidth = rawTarget;
      cssHeight = rawTarget * currentRatio;
    }
    scheduleRender();
  }

  onDestroy(() => {
    activeRenderId += 1;

    if (renderDebounce) {
      clearTimeout(renderDebounce);
      renderDebounce = null;
    }
  });
</script>

<article class="page" aria-label={`Page ${pageNumber}`} style={`width:${cssWidth}px;min-height:${cssHeight}px`}>
  {#if imageUrl}
    <img
      class="page-image"
      src={imageUrl}
      alt={`Page ${pageNumber}`}
      loading="lazy"
      decoding="async"
      draggable="false"
    />
  {:else}
    <div class="placeholder" style={`height:${cssHeight}px`} aria-hidden="true"></div>
  {/if}

  {#if errorMessage}
    <div class="error">{errorMessage}</div>
  {/if}
</article>

<style>
  .page {
    position: relative;
    box-sizing: border-box;
    border-radius: 0;
    overflow: hidden;
    background: white;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.06);
    transform: translateZ(0);
  }

  .page-image {
    display: block;
    width: 100%;
    height: auto;
    background: white;
    transform: translateZ(0);
  }

  .placeholder {
    width: 100%;
    background: white;
    animation: viewer-pulse 2s ease-in-out infinite alternate;
  }

  @keyframes viewer-pulse {
    0% { opacity: 0.5; }
    100% { opacity: 0.95; }
  }

  .error {
    position: absolute;
    left: 0.45rem;
    bottom: 0.45rem;
    padding: 0.16rem 0.38rem;
    font-size: 0.72rem;
    color: #781515;
    background: rgb(255 237 237 / 0.96);
    border-color: rgb(203 98 98 / 0.8);
    border: 1px solid;
    border-radius: 0.24rem;
  }

  @media (prefers-reduced-motion: reduce) {
    .placeholder {
      animation: none;
      opacity: 0.85;
    }
  }
</style>
