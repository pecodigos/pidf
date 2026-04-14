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
  let errorDetails = "";
  let statusMessage = "";
  let copyFeedback = "";
  let showSidebar = false;
  let showFindPanel = false;
  let findPageField = "1";
  let findPanelInput: HTMLInputElement | null = null;
  let findPanelElement: HTMLDivElement | null = null;
  let lastAttemptedPath: string | null = null;
  let activeOpenRequestId = 0;
  const appWindow = getCurrentWindow();
  const OPEN_PDF_TIMEOUT_MS = 45000;
  const FIRST_RENDER_TIMEOUT_MS = 12000;
  const SESSION_DESTROY_TIMEOUT_MS = 5000;
  const THEME_STORAGE_KEY = "pidf.theme";

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

  function readStoredThemePreference(): boolean | null {
    try {
      const storedValue = window.localStorage.getItem(THEME_STORAGE_KEY);
      if (storedValue === "dark") {
        return true;
      }

      if (storedValue === "light") {
        return false;
      }
    } catch (error) {
      console.warn("[PiDF] failed to read stored theme preference", error);
    }

    return null;
  }

  function persistThemePreference(nextIsDark: boolean): void {
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, nextIsDark ? "dark" : "light");
    } catch (error) {
      console.warn("[PiDF] failed to persist theme preference", error);
    }
  }

  function applyInitialThemePreference(): void {
    const storedPreference = readStoredThemePreference();
    if (storedPreference !== null) {
      darkMode.set(storedPreference);
      return;
    }

    darkMode.set(window.matchMedia("(prefers-color-scheme: dark)").matches);
  }

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

  function openFindPanel(): void {
    if ($pageCount <= 0) {
      return;
    }

    showFindPanel = true;
    findPageField = String($currentPage);
    window.requestAnimationFrame(() => {
      findPanelInput?.focus();
      findPanelInput?.select();
    });
  }

  function closeFindPanel(): void {
    showFindPanel = false;
  }

  function submitFindPanelJump(): void {
    const parsedPage = Number.parseInt(findPageField, 10);
    if (!Number.isFinite(parsedPage)) {
      findPageField = String($currentPage);
      return;
    }

    jumpToPage(parsedPage);
    closeFindPanel();
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
      showFindPanel = false;
      setStatusMessage(`Opened ${extractFileName(selectedPath)}.`);

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
      persistThemePreference(nextValue);
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
      if (!showFindPanel || !findPanelElement) {
        return;
      }

      const target = event.target;
      if (target instanceof Node && !findPanelElement.contains(target)) {
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
    <div
      class="find-panel"
      bind:this={findPanelElement}
      role="dialog"
      aria-label="Find and jump"
      aria-modal="false"
    >
      <div class="find-header">
        <h2>Find</h2>
        <button
          type="button"
          class="find-close"
          on:click={closeFindPanel}
          aria-label="Close find panel"
        >
          Close
        </button>
      </div>

      <p class="find-hint">Text search is not available in image mode yet. Jump to a page instead.</p>

      <form class="find-form" on:submit|preventDefault={submitFindPanelJump}>
        <label class="find-field" for="find-page-input">
          Page
        </label>
        <input
          id="find-page-input"
          bind:this={findPanelInput}
          bind:value={findPageField}
          type="number"
          min="1"
          max={Math.max(1, $pageCount)}
          inputmode="numeric"
          aria-label="Jump to page"
        />

        <button type="submit">Go</button>
      </form>
    </div>
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
    <div class="error" role="alert">
      <p>{errorMessage}</p>
      <div class="error-actions">
        <button on:click={() => void retryLastOpen()} disabled={loading || !lastAttemptedPath}>
          Retry
        </button>
        <button on:click={() => void openPdf()} disabled={loading}>Open Another PDF</button>
        <button on:click={() => void copyErrorDetails()} disabled={!errorDetails}>Copy Details</button>
        {#if copyFeedback}
          <span class="error-feedback" role="status" aria-live="polite">{copyFeedback}</span>
        {/if}
      </div>
    </div>
  {/if}
</main>

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

  .find-panel {
    position: fixed;
    top: 4.35rem;
    right: 1rem;
    z-index: 40;
    width: min(24rem, calc(100vw - 2rem));
    border: 1px solid var(--line);
    border-radius: 0.8rem;
    background: var(--panel);
    color: var(--text);
    box-shadow: 0 20px 32px rgb(0 0 0 / 0.18);
    padding: 0.85rem;
    display: grid;
    gap: 0.7rem;
    animation: find-panel-enter 140ms ease-out;
  }

  @keyframes find-panel-enter {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.985);
    }

    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .find-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .find-header h2 {
    margin: 0;
    font-size: 0.98rem;
    letter-spacing: 0.01em;
  }

  .find-hint {
    margin: 0;
    color: var(--muted);
    font-size: 0.86rem;
    line-height: 1.35;
  }

  .find-form {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 0.55rem;
    align-items: center;
  }

  .find-field {
    font-size: 0.88rem;
    color: var(--muted);
  }

  .find-form input,
  .find-form button,
  .find-close {
    border: 1px solid var(--line);
    border-radius: 0.6rem;
    background: var(--panel-raised);
    color: var(--text);
    font: inherit;
    min-height: 2.35rem;
  }

  .find-form input {
    padding: 0 0.65rem;
  }

  .find-form button,
  .find-close {
    padding: 0 0.8rem;
    cursor: pointer;
    transition: border-color 120ms ease, box-shadow 120ms ease, background-color 120ms ease;
  }

  .find-form button:hover,
  .find-close:hover {
    border-color: color-mix(in oklab, var(--accent) 36%, var(--line));
  }

  .find-form button:focus-visible,
  .find-form input:focus-visible,
  .find-close:focus-visible {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 60%, var(--line));
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 28%, transparent);
  }

  .workbench.with-sidebar {
    grid-template-columns: auto 1fr;
  }

  .error {
    margin: 0;
    padding: 0.7rem 0.9rem;
    display: grid;
    gap: 0.55rem;
    color: #ffced1;
    background: rgb(123 24 24 / 0.92);
    border-top: 1px solid rgb(242 108 108 / 0.5);
  }

  .error p {
    margin: 0;
    font-size: 0.88rem;
  }

  .error-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .error-actions button {
    border: 1px solid rgb(242 108 108 / 0.7);
    border-radius: 0.5rem;
    background: rgb(93 17 17 / 0.55);
    color: #ffe0e3;
    font: inherit;
    font-size: 0.82rem;
    min-height: 2rem;
    padding: 0 0.75rem;
    cursor: pointer;
    transition: background-color 120ms ease, box-shadow 120ms ease;
  }

  .error-actions button:hover:not(:disabled) {
    background: rgb(111 24 24 / 0.65);
  }

  .error-actions button:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px rgb(255 193 198 / 0.45);
  }

  .error-actions button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .error-feedback {
    align-self: center;
    font-size: 0.8rem;
    color: #ffd6da;
    margin-left: 0.25rem;
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

  @media (prefers-reduced-motion: reduce) {
    .find-panel {
      animation: none;
    }

    .find-form button,
    .find-close,
    .error-actions button {
      transition: none;
    }
  }

  @media (max-width: 860px) {
    .find-panel {
      top: 8rem;
      left: 1rem;
      right: 1rem;
      width: auto;
    }

    .find-form {
      grid-template-columns: 1fr;
    }

    .find-field {
      display: none;
    }

    .workbench {
      grid-template-columns: 1fr;
    }
  }
</style>
