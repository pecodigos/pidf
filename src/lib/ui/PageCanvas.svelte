<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";

  import { cacheKey, type RenderCache } from "$lib/core/renderCache";
  import type { PdfSession } from "$lib/core/pdf";
  import { currentPage as viewerCurrentPage } from "$lib/state/viewer";
  import { logPdfStage } from "$lib/core/trace";

  export let session: PdfSession;
  export let pageNumber: number;
  export let targetWidth = 900;
  export let cache: RenderCache;

  const dispatch = createEventDispatcher<{
    rendercommitted: { pageNumber: number };
    rendererror: { pageNumber: number; message: string };
  }>();

  const DEFAULT_RATIO = Math.SQRT2;
  const RENDER_TIMEOUT_MS = 12000;
  const INITIAL_RENDER_DEBOUNCE_MS = 40;
  const VIEWPORT_PRIORITY_DISTANCE = 1;
  const HIGH_PRIORITY_RENDER_DEBOUNCE_MS = 16;
  const LOW_PRIORITY_RENDER_DEBOUNCE_MS = 260;
  const RESIZE_RENDER_DEBOUNCE_MS = 120;
  const OFFSCREEN_RESIZE_RENDER_DEBOUNCE_MS = 420;
  const ENABLE_PAGE_DIAGNOSTICS = false;

  let imageUrl = "";
  let cssWidth = Math.max(1, targetWidth);
  let cssHeight = Math.max(120, targetWidth * DEFAULT_RATIO);
  let errorMessage = "";
  let lastRenderKey = "";
  let activeRenderId = 0;
  let activeBlobUrl = "";
  let attemptedFallbackForUrl = "";
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

  async function withTimeout<T>(
    promise: Promise<T>,
    timeoutMs: number,
    message: string,
  ): Promise<T> {
    let timeoutId: ReturnType<typeof setTimeout> | null = null;

    try {
      return await Promise.race([
        promise,
        new Promise<never>((_, reject) => {
          timeoutId = setTimeout(() => reject(new Error(message)), timeoutMs);
        }),
      ]);
    } finally {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
    }
  }

  function clearActiveBlobUrl(): void {
    if (!activeBlobUrl) {
      return;
    }

    URL.revokeObjectURL(activeBlobUrl);
    activeBlobUrl = "";
  }

  function setDisplayedImageUrl(nextUrl: string): void {
    if (imageUrl !== nextUrl) {
      clearActiveBlobUrl();
    }

    imageUrl = nextUrl;

    if (nextUrl.startsWith("blob:")) {
      activeBlobUrl = nextUrl;
    }
  }

  async function fallbackToBlobUrl(sourceUrl: string, renderId: number): Promise<void> {
    const response = await fetch(sourceUrl, { method: "GET" });
    if (!response.ok) {
      throw new Error(`Asset fetch failed with HTTP ${response.status}.`);
    }

    const blob = await response.blob();
    if (!blob.size) {
      throw new Error("Asset fetch returned an empty payload.");
    }

    if (renderId !== activeRenderId) {
      return;
    }

    setDisplayedImageUrl(URL.createObjectURL(blob));
  }

  function handleImageError(): void {
    if (!imageUrl || imageUrl.startsWith("blob:") || attemptedFallbackForUrl === imageUrl) {
      return;
    }

    const failedUrl = imageUrl;
    const renderId = activeRenderId;
    attemptedFallbackForUrl = failedUrl;

    void fallbackToBlobUrl(failedUrl, renderId).catch((error) => {
      if (renderId !== activeRenderId) {
        return;
      }

      const describedError = describeError(error);
      errorMessage = `Page ${pageNumber}: ${describedError.message}`;

      dispatch("rendererror", {
        pageNumber,
        message: describedError.message,
      });

      logRenderStage("page_render_failed", {
        targetWidth: Math.max(1, Math.round(targetWidth)),
        message: describedError.message,
      });
    });
  }

  async function renderPage(): Promise<void> {
    if (!session || targetWidth <= 0) {
      return;
    }

    const renderWidth = Math.max(1, Math.round(targetWidth));
    const currentKey = cacheKey(pageNumber, renderWidth);
    if (currentKey === lastRenderKey && !errorMessage) {
      return;
    }

    const renderId = ++activeRenderId;
    errorMessage = "";

    cssWidth = renderWidth;
    cssHeight = Math.max(120, renderWidth * DEFAULT_RATIO);

    try {
      const cachedEntry = cache.get(currentKey);
      if (cachedEntry) {
        logRenderStage("page_render_cache_hit", {
          targetWidth: renderWidth,
        });

        if (renderId !== activeRenderId) {
          return;
        }

        attemptedFallbackForUrl = "";
        setDisplayedImageUrl(cachedEntry.imageUrl);
        cssWidth = cachedEntry.cssWidth;
        cssHeight = cachedEntry.cssHeight;

        dispatch("rendercommitted", {
          pageNumber,
        });

        lastRenderKey = currentKey;
        return;
      }

      if (ENABLE_PAGE_DIAGNOSTICS) {
        console.info("[PiDF] page render started", {
          pageNumber,
          targetWidth: renderWidth,
          renderEngine: session.diagnostics.renderEngine,
        });
      }

      logRenderStage("page_render_start", {
        targetWidth: renderWidth,
      });

      const renderPriority = Math.min(4096, Math.abs($viewerCurrentPage - pageNumber));

      const rendered = await withTimeout(
        session.renderPage(pageNumber, renderWidth, renderPriority),
        RENDER_TIMEOUT_MS,
        `Backend page render timed out after ${RENDER_TIMEOUT_MS}ms.`,
      );

      if (ENABLE_PAGE_DIAGNOSTICS) {
        console.info("[PiDF] page render payload", {
          pageNumber,
          imageUrl: rendered.imageUrl,
          width: rendered.cssWidth,
          height: rendered.cssHeight,
        });
      }

      if (renderId !== activeRenderId) {
        return;
      }

      attemptedFallbackForUrl = "";
      setDisplayedImageUrl(rendered.imageUrl);
      cssWidth = rendered.cssWidth;
      cssHeight = rendered.cssHeight;

      cache.set(currentKey, {
        imageUrl: rendered.imageUrl,
        cssWidth,
        cssHeight,
      });

      dispatch("rendercommitted", {
        pageNumber,
      });

      logRenderStage("page_render_committed", {
        targetWidth: renderWidth,
      });

      if (ENABLE_PAGE_DIAGNOSTICS) {
        console.info("[PiDF] page render committed", {
          pageNumber,
          imageUrl,
          width: cssWidth,
          height: cssHeight,
        });
      }

      lastRenderKey = currentKey;
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
        targetWidth: renderWidth,
        message: describedError.message,
      });

      console.error("[PiDF] page render failed", {
        pageNumber,
        targetWidth: renderWidth,
        imageUrl,
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

    const distance = Math.abs($viewerCurrentPage - pageNumber);
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

  $: if (session && targetWidth > 0 && $viewerCurrentPage >= 0) {
    scheduleRender();
  }

  onDestroy(() => {
    activeRenderId += 1;

    if (renderDebounce) {
      clearTimeout(renderDebounce);
      renderDebounce = null;
    }

    clearActiveBlobUrl();
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
      on:error={handleImageError}
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
    border: 1px solid rgb(0 0 0 / 0.14);
    border-radius: 0.2rem;
    overflow: hidden;
    background: white;
  }

  .page-image {
    display: block;
    width: 100%;
    height: auto;
    background: white;
  }

  .placeholder {
    width: 100%;
    background: white;
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
</style>
