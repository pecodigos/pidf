<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onDestroy, onMount } from "svelte";

  import { createPdfSession, readInitialPdfPath, type PdfSession } from "$lib/core/pdf";
  import { withTimeout } from "$lib/core/async";
  import {
    isEditableTarget,
    isFindShortcut,
    isFullscreenShortcut,
    isNextPageShortcut,
    isOpenShortcut,
    isPreviousPageShortcut,
    isZoomInShortcut,
    isZoomOutShortcut,
    isZoomResetShortcut,
  } from "$lib/core/keyboard";
  import { extractFileName } from "$lib/core/path";
  import {
    applyThemePreference,
    persistThemePreference as persistThemePreferenceToStorage,
  } from "$lib/core/themePreference";
  import { logPdfStage } from "$lib/core/trace";
  import {
    currentPage,
    darkMode,
    fileName,
    pageCount,
    resetZoom,
    setCurrentPage,
    setPageCount,
    updateZoom,
    zoom,
    zoomIn,
    zoomOut,
  } from "$lib/state/viewer";
  import Sidebar from "$lib/ui/Sidebar.svelte";
  import Toolbar from "$lib/ui/Toolbar.svelte";
  import VirtualPdfViewer from "$lib/ui/VirtualPdfViewer.svelte";
  import ErrorBanner from "$lib/ui/ErrorBanner.svelte";
  import FindPanel from "$lib/ui/FindPanel.svelte";

  let viewer: VirtualPdfViewer | null = null;
  let session: PdfSession | null = null;

  let loading = false;
  let errorMessage = "";
  let errorDetails = "";
  let statusMessage = "";
  let copyFeedback = "";
  let showSidebar = false;
  let showFindPanel = false;
  let findPanelComponent: FindPanel | null = null;
  let lastAttemptedPath: string | null = null;
  let activeOpenRequestId = 0;
  const appWindow = getCurrentWindow();
  const OPEN_PDF_TIMEOUT_MS = 45000;
  const FIRST_RENDER_TIMEOUT_MS = 12000;
  const SESSION_DESTROY_TIMEOUT_MS = 5000;

  type PendingFirstRender = {
    attemptId: string;
    resolve: () => void;
    reject: (error: Error) => void;
  };

  let pendingFirstRender: PendingFirstRender | null = null;
  let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null;

  function setStatusMessage(message: string): void {
    statusMessage = message;
  }

  function clearCopyFeedbackTimer(): void {
    if (!copyFeedbackTimer) {
      return;
    }

    clearTimeout(copyFeedbackTimer);
    copyFeedbackTimer = null;
  }

  function showCopyFeedback(message: string): void {
    clearCopyFeedbackTimer();
    copyFeedback = message;

    copyFeedbackTimer = setTimeout(() => {
      copyFeedback = "";
      copyFeedbackTimer = null;
    }, 1800);
  }

  function applyInitialThemePreference(): void {
    darkMode.set(applyThemePreference(window.localStorage, window.matchMedia));
  }

  function waitForFirstRender(attemptId: string): Promise<void> {
    if (pendingFirstRender) {
      pendingFirstRender.reject(
        new Error("First-render wait superseded by a newer open request."),
      );
      pendingFirstRender = null;
    }

    return new Promise((resolve, reject) => {
      pendingFirstRender = {
        attemptId,
        resolve,
        reject,
      };
    });
  }

  function clearPendingFirstRender(attemptId: string): void {
    if (pendingFirstRender?.attemptId === attemptId) {
      pendingFirstRender = null;
    }
  }

  async function destroySessionSafely(targetSession: PdfSession | null): Promise<void> {
    if (!targetSession) {
      return;
    }

    try {
      await withTimeout(
        targetSession.destroy(),
        SESSION_DESTROY_TIMEOUT_MS,
        `Session destroy timed out after ${SESSION_DESTROY_TIMEOUT_MS}ms.`,
      );
    } catch (error) {
      console.warn("[PiDF] session destroy failed or timed out", error);
    }
  }

  async function toggleFullscreenShortcut(): Promise<void> {
    try {
      const isFullscreen = await appWindow.isFullscreen();
      await appWindow.setFullscreen(!isFullscreen);
    } catch (error) {
      console.warn("[PiDF] failed to toggle fullscreen", error);
    }
  }

  function openFindPanel(): void {
    if ($pageCount <= 0) {
      return;
    }
    showFindPanel = true;
  }

  function closeFindPanel(): void {
    showFindPanel = false;
  }

  function monitorFirstRender(nextSession: PdfSession, selectedPath: string): void {
    const attemptId = nextSession.diagnostics.openAttemptId;

    logPdfStage(
      "ui_wait_first_render_start",
      {
        openAttemptId: attemptId,
        sourceMode: nextSession.diagnostics.sourceMode,
      },
      Date.now() - nextSession.diagnostics.openStartedAtMs,
    );

    void withTimeout(
      waitForFirstRender(attemptId),
      FIRST_RENDER_TIMEOUT_MS,
      `First page render timed out after ${FIRST_RENDER_TIMEOUT_MS}ms.`,
    )
      .then(() => {
        logPdfStage(
          "ui_open_succeeded",
          {
            openAttemptId: attemptId,
            pageCount: nextSession.pageCount,
            sourceMode: nextSession.diagnostics.sourceMode,
          },
          Date.now() - nextSession.diagnostics.openStartedAtMs,
        );
      })
      .catch((error) => {
        clearPendingFirstRender(attemptId);

        if (session !== nextSession) {
          return;
        }

        const message = error instanceof Error ? error.message : String(error);
        errorMessage = `Opened PDF but first page failed to render. ${message} Try reopening the file.`;
        errorDetails = `Path: ${selectedPath}\nOpen attempt: ${attemptId}\nError: ${message}`;
        setStatusMessage("Opened PDF, but first page render failed.");

        logPdfStage("ui_open_failed", {
          path: selectedPath,
          openAttemptId: attemptId,
          error: message,
        });
      });
  }

  async function loadPdfPath(selectedPath: string): Promise<void> {
    const openRequestId = ++activeOpenRequestId;
    lastAttemptedPath = selectedPath;

    loading = true;
    errorMessage = "";
    errorDetails = "";
    copyFeedback = "";
    setStatusMessage("Opening PDF...");

    let nextSession: PdfSession | null = null;

    try {
      nextSession = await withTimeout(
        createPdfSession(selectedPath),
        OPEN_PDF_TIMEOUT_MS,
        `Opening PDF timed out after ${OPEN_PDF_TIMEOUT_MS}ms.`,
      );

      if (openRequestId !== activeOpenRequestId) {
        if (nextSession) {
          await destroySessionSafely(nextSession);
        }

        return;
      }

      const previousSession = session;

      session = nextSession;

      const openedFileName = extractFileName(selectedPath);

      fileName.set(openedFileName);
      setPageCount(nextSession.pageCount);
      setCurrentPage(1);
      showFindPanel = false;
      setStatusMessage(`Opened ${openedFileName}.`);

      monitorFirstRender(nextSession, selectedPath);
      void destroySessionSafely(previousSession);
      viewer?.jumpToPage(1);
    } catch (error) {
      if (openRequestId !== activeOpenRequestId) {
        if (nextSession) {
          void destroySessionSafely(nextSession);
        }

        return;
      }

      const message = error instanceof Error ? error.message : String(error);

      if (nextSession) {
        void destroySessionSafely(nextSession);
      }

      errorMessage = `Unable to open PDF. ${message} Check that the file is readable and try again.`;
      errorDetails = `Path: ${selectedPath}\nError: ${message}`;
      setStatusMessage("Unable to open PDF.");
      logPdfStage("ui_open_failed", {
        path: selectedPath,
        openAttemptId: nextSession?.diagnostics.openAttemptId ?? null,
        error: message,
      });
    } finally {
      if (openRequestId === activeOpenRequestId) {
        loading = false;
      }
    }
  }

  async function openPdf(): Promise<void> {
    if (loading) {
      return;
    }

    const selectedPath = await open({
      title: "Open PDF",
      directory: false,
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });

    if (!selectedPath || Array.isArray(selectedPath)) {
      return;
    }

    await loadPdfPath(selectedPath);
  }

  async function retryLastOpen(): Promise<void> {
    if (!lastAttemptedPath || loading) {
      return;
    }

    await loadPdfPath(lastAttemptedPath);
  }

  async function copyErrorDetails(): Promise<void> {
    if (!errorDetails) {
      return;
    }

    try {
      await navigator.clipboard.writeText(errorDetails);
      showCopyFeedback("Copied details.");
    } catch (error) {
      console.warn("[PiDF] failed to copy error details", error);
      showCopyFeedback("Copy failed.");
    }
  }

  function jumpToPage(page: number): void {
    if ($pageCount <= 0) {
      return;
    }

    const normalizedPage = Math.max(1, Math.min($pageCount, Math.floor(page)));
    viewer?.jumpToPage(normalizedPage);
    setCurrentPage(normalizedPage);
  }

  function toggleTheme(): void {
    darkMode.update((value) => {
      const nextValue = !value;
      persistThemePreferenceToStorage(window.localStorage, nextValue);
      return nextValue;
    });
  }

  function handleViewerPageChange(event: CustomEvent<{ page: number }>): void {
    setCurrentPage(event.detail.page);
  }

  function handleViewerZoomChange(event: CustomEvent<{ zoom: number }>): void {
    updateZoom(event.detail.zoom);
  }

  function handleViewerFirstRender(event: CustomEvent<{ attemptId: string; page: number }>): void {
    if (!pendingFirstRender || pendingFirstRender.attemptId !== event.detail.attemptId) {
      return;
    }

    pendingFirstRender.resolve();
    pendingFirstRender = null;
  }

  function handleViewerRenderError(
    event: CustomEvent<{ attemptId: string; page: number; message: string }>,
  ): void {
    if (!pendingFirstRender || pendingFirstRender.attemptId !== event.detail.attemptId) {
      return;
    }

    pendingFirstRender.reject(
      new Error(`First page render failed: ${event.detail.message}`),
    );
    pendingFirstRender = null;
  }

  onMount(() => {
    applyInitialThemePreference();

    const onWindowPointerDown = (event: PointerEvent) => {
      if (!showFindPanel || !findPanelComponent) {
        return;
      }

      const target = event.target;
      if (target instanceof Node && !findPanelComponent.contains(target)) {
        closeFindPanel();
      }
    };

    const onWindowKeydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) {
        return;
      }

      if (isOpenShortcut(event)) {
        event.preventDefault();
        void openPdf();
        return;
      }

      if (isFindShortcut(event)) {
        event.preventDefault();
        openFindPanel();
        return;
      }

      if (showFindPanel && event.key === "Escape") {
        event.preventDefault();
        closeFindPanel();
        return;
      }

      if (isFullscreenShortcut(event)) {
        event.preventDefault();
        void toggleFullscreenShortcut();
        return;
      }

      if (isEditableTarget(event.target)) {
        return;
      }

      if (showFindPanel) {
        return;
      }

      if (isNextPageShortcut(event)) {
        event.preventDefault();
        jumpToPage($currentPage + 1);
        return;
      }

      if (isPreviousPageShortcut(event)) {
        event.preventDefault();
        jumpToPage($currentPage - 1);
        return;
      }

      if (isZoomInShortcut(event)) {
        event.preventDefault();
        zoomIn();
        return;
      }

      if (isZoomOutShortcut(event)) {
        event.preventDefault();
        zoomOut();
        return;
      }

      if (isZoomResetShortcut(event)) {
        event.preventDefault();
        resetZoom();
      }
    };

    window.addEventListener("pointerdown", onWindowPointerDown);
    window.addEventListener("keydown", onWindowKeydown);

    void readInitialPdfPath()
      .then((initialPath) => {
        if (initialPath) {
          void loadPdfPath(initialPath);
        }
      })
      .catch((error) => {
        console.warn("[PiDF] failed to read initial PDF path", error);
      });

    return () => {
      window.removeEventListener("pointerdown", onWindowPointerDown);
      window.removeEventListener("keydown", onWindowKeydown);
    };
  });

  $: if (showFindPanel && $pageCount <= 0) {
    closeFindPanel();
  }

  onDestroy(() => {
    clearCopyFeedbackTimer();
    pendingFirstRender = null;
    void destroySessionSafely(session);
  });
</script>

<main class="app" class:dark={$darkMode} aria-busy={loading}>
  <p class="sr-only" role="status" aria-live="polite">{loading ? "Opening PDF..." : statusMessage}</p>

  <Toolbar
    fileName={$fileName}
    currentPage={$currentPage}
    pageCount={$pageCount}
    zoom={$zoom}
    themeDark={$darkMode}
    {loading}
    {showSidebar}
    on:open={openPdf}
    on:find={openFindPanel}
    on:jump={(event) => jumpToPage(event.detail.page)}
    on:zoomin={zoomIn}
    on:zoomout={zoomOut}
    on:zoomreset={resetZoom}
    on:togglesidebar={() => (showSidebar = !showSidebar)}
    on:toggletheme={toggleTheme}
  />

  {#if showFindPanel}
    <FindPanel
      bind:this={findPanelComponent}
      pageCount={$pageCount}
      currentPage={$currentPage}
      on:close={closeFindPanel}
      on:jump={(event) => jumpToPage(event.detail.page)}
    />
  {/if}

  <div class="workbench" class:with-sidebar={showSidebar}>
    {#if showSidebar}
      <Sidebar
        pageCount={$pageCount}
        currentPage={$currentPage}
        on:jump={(event) => jumpToPage(event.detail.page)}
      />
    {/if}

    <VirtualPdfViewer
      bind:this={viewer}
      {session}
      zoom={$zoom}
      on:pagechange={handleViewerPageChange}
      on:zoomchange={handleViewerZoomChange}
      on:firstrender={handleViewerFirstRender}
      on:rendererror={handleViewerRenderError}
    />
  </div>

  {#if errorMessage}
    <ErrorBanner
      message={errorMessage}
      hasDetails={!!errorDetails}
      {copyFeedback}
      {loading}
      canRetry={!!lastAttemptedPath}
      on:retry={() => void retryLastOpen()}
      on:open={() => void openPdf()}
      on:copydetails={() => void copyErrorDetails()}
    />
  {/if}
</main>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    font-family: "Inter", "Segoe UI", "Noto Sans", sans-serif;
    text-rendering: geometricPrecision;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(::-webkit-scrollbar) {
    width: 12px;
    height: 12px;
  }

  :global(::-webkit-scrollbar-track) {
    background: transparent;
  }

  :global(::-webkit-scrollbar-thumb) {
    background: color-mix(in oklab, var(--muted) 40%, transparent);
    border: 3px solid transparent;
    background-clip: padding-box;
    border-radius: 9999px;
  }

  :global(::-webkit-scrollbar-thumb:hover) {
    background: color-mix(in oklab, var(--muted) 60%, transparent);
    border: 3px solid transparent;
    background-clip: padding-box;
  }

  :global(body) {
    background: #eceff4;
  }

  .app {
    --bg: #f4f6f8;
    --panel: rgb(255 255 255 / 0.96);
    --panel-raised: #ffffff;
    --text: #171a1c;
    --muted: #5e656d;
    --line: rgb(0 0 0 / 0.08);
    --accent: #3b82f6;

    width: 100vw;
    height: 100vh;
    display: grid;
    grid-template-rows: auto 1fr auto;
    color: var(--text);
    background: var(--bg);
  }

  .app.dark {
    --bg: #0b0d0f;
    --panel: rgb(22 25 29 / 0.96);
    --panel-raised: rgb(30 35 40 / 1);
    --text: #dde2e8;
    --muted: #7f8a96;
    --line: rgb(255 255 255 / 0.08);
    --accent: #60a5fa;
  }

  .workbench {
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr;
    border-top: 1px solid color-mix(in oklab, var(--line) 40%, transparent);
  }

  .workbench.with-sidebar {
    grid-template-columns: auto 1fr;
  }



  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  @media (max-width: 860px) {
    .workbench {
      grid-template-columns: 1fr;
    }
  }
</style>
