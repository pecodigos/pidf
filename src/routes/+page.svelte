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

  let viewer: VirtualPdfViewer | null = null;
  let session: PdfSession | null = null;

  let loading = false;
  let errorMessage = "";
  let showSidebar = false;
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

  function extractFileName(path: string): string {
    const parts = path.split(/[\\/]/);
    return parts[parts.length - 1] || path;
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

  function openFindShortcut(): void {
    const response = window.prompt(
      "Find text is not available in image mode yet. Enter a page number to jump:",
      String($currentPage),
    );

    if (!response) {
      return;
    }

    const parsedPage = Number.parseInt(response, 10);
    if (Number.isFinite(parsedPage)) {
      jumpToPage(parsedPage);
    }
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

        logPdfStage("ui_open_failed", {
          path: selectedPath,
          openAttemptId: attemptId,
          error: message,
        });
      });
  }

  async function loadPdfPath(selectedPath: string): Promise<void> {
    const openRequestId = ++activeOpenRequestId;

    loading = true;
    errorMessage = "";

    console.info("[PiDF] opening selected PDF", { path: selectedPath });

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

      fileName.set(extractFileName(selectedPath));
      setPageCount(nextSession.pageCount);
      setCurrentPage(1);

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
      defaultPath: "/home/pepe/Documents",
      directory: false,
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });

    if (!selectedPath || Array.isArray(selectedPath)) {
      return;
    }

    await loadPdfPath(selectedPath);
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
    darkMode.update((value) => !value);
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
    const prefersDarkMode = window.matchMedia("(prefers-color-scheme: dark)").matches;
    darkMode.set(prefersDarkMode);

    const onWindowKeydown = (event: KeyboardEvent) => {
      if (isOpenShortcut(event)) {
        event.preventDefault();
        void openPdf();
        return;
      }

      if (isFindShortcut(event)) {
        event.preventDefault();
        openFindShortcut();
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
      window.removeEventListener("keydown", onWindowKeydown);
    };
  });

  onDestroy(() => {
    pendingFirstRender = null;
    void destroySessionSafely(session);
  });
</script>

<div class="app" class:dark={$darkMode}>
  <Toolbar
    fileName={$fileName}
    currentPage={$currentPage}
    pageCount={$pageCount}
    zoom={$zoom}
    {loading}
    {showSidebar}
    on:open={openPdf}
    on:jump={(event) => jumpToPage(event.detail.page)}
    on:zoomin={zoomIn}
    on:zoomout={zoomOut}
    on:zoomreset={resetZoom}
    on:togglesidebar={() => (showSidebar = !showSidebar)}
    on:toggletheme={toggleTheme}
  />

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
    <div class="error" role="alert">
      {errorMessage}
    </div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    font-family: "IBM Plex Sans", "Segoe UI", "Noto Sans", sans-serif;
    text-rendering: geometricPrecision;
  }

  :global(body) {
    background: #eceff4;
  }

  .app {
    --bg: #eceff4;
    --panel: #e4e9f1;
    --panel-raised: #f5f8ff;
    --text: #16202a;
    --muted: #546376;
    --line: #bdc8d6;
    --accent: #1c6fbf;

    width: 100vw;
    height: 100vh;
    display: grid;
    grid-template-rows: auto 1fr auto;
    color: var(--text);
    background: var(--bg);
  }

  .app.dark {
    --bg: #0f141a;
    --panel: #131b24;
    --panel-raised: #1a2431;
    --text: #d9e4ef;
    --muted: #8c9fb4;
    --line: #2a3949;
    --accent: #4ea3ed;
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

  .error {
    margin: 0;
    padding: 0.62rem 0.9rem;
    font-size: 0.88rem;
    color: #ffced1;
    background: rgb(123 24 24 / 0.92);
    border-top: 1px solid rgb(242 108 108 / 0.5);
  }

  @media (max-width: 860px) {
    .workbench {
      grid-template-columns: 1fr;
    }
  }
</style>
