import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { createPdfOpenTrace } from "./trace";

const MIN_PAGE_NUMBER = 1;
const FILE_READ_TIMEOUT_MS = 15000;
const DOCUMENT_LOAD_TIMEOUT_MS = 30000;
const PAGE_RENDER_TIMEOUT_MS = 15000;
const MIN_TARGET_WIDTH = 240;
const MAX_TARGET_WIDTH = 2200;
const ENABLE_RENDER_DIAGNOSTICS = false;

type PdfSourceMode = "tauri-pdfium";

interface PdfOpenInfo {
  sessionId: string;
  pageCount: number;
  firstPageWidth: number;
  firstPageHeight: number;
  renderEngine: string;
}

interface PdfRenderedPagePayload {
  imagePath: string;
  width: number;
  height: number;
}

export interface PdfRenderedPage {
  imageUrl: string;
  cssWidth: number;
  cssHeight: number;
}

export interface PdfSessionDiagnostics {
  renderEngine: "pdfium-render";
  openAttemptId: string;
  openStartedAtMs: number;
  sourceMode: PdfSourceMode;
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

function normalizeTargetWidth(targetWidth: number): number {
  return Math.round(Math.max(MIN_TARGET_WIDTH, Math.min(MAX_TARGET_WIDTH, targetWidth || 0)));
}

export class PdfSession {
  readonly pageCount: number;
  readonly diagnostics: PdfSessionDiagnostics;

  #sessionId: string;
  #firstPageRatio: number;
  #renderCache = new Map<string, Promise<PdfRenderedPage>>();
  #destroyed = false;

  constructor(
    sessionId: string,
    pageCount: number,
    firstPageRatio: number,
    diagnostics: PdfSessionDiagnostics,
  ) {
    this.#sessionId = sessionId;
    this.pageCount = pageCount;
    this.#firstPageRatio = firstPageRatio;
    this.diagnostics = diagnostics;
  }

  async renderPage(
    pageNumber: number,
    targetWidth: number,
    renderPriority = 100,
  ): Promise<PdfRenderedPage> {
    if (this.#destroyed) {
      throw new Error("PDF session was already destroyed.");
    }

    const normalizedPage = Math.max(
      MIN_PAGE_NUMBER,
      Math.min(this.pageCount, Math.floor(pageNumber)),
    );
    const normalizedWidth = normalizeTargetWidth(targetWidth);
    const normalizedPriority = Math.max(0, Math.min(4096, Math.floor(renderPriority || 0)));
    const cacheKey = `${normalizedPage}:${normalizedWidth}`;

    const existing = this.#renderCache.get(cacheKey);
    if (existing) {
      return existing;
    }

    const next = withTimeout(
      invoke<PdfRenderedPagePayload>("pdf_render_page", {
        sessionId: this.#sessionId,
        pageNumber: normalizedPage,
        targetWidth: normalizedWidth,
        renderPriority: normalizedPriority,
      }),
      PAGE_RENDER_TIMEOUT_MS,
      `PDF render timed out after ${PAGE_RENDER_TIMEOUT_MS}ms (page ${normalizedPage}).`,
    )
      .then((payload) => {
        const imageUrl = convertFileSrc(payload.imagePath);

        if (ENABLE_RENDER_DIAGNOSTICS) {
          console.info("[PiDF] backend render payload", {
            pageNumber: normalizedPage,
            targetWidth: normalizedWidth,
            imagePath: payload.imagePath,
            imageUrl,
            width: payload.width,
            height: payload.height,
          });
        }

        return {
          imageUrl,
          cssWidth: Math.max(1, payload.width || normalizedWidth),
          cssHeight: Math.max(1, payload.height || normalizedWidth * this.#firstPageRatio),
        };
      })
      .catch((error) => {
        this.#renderCache.delete(cacheKey);
        throw error;
      });

    this.#renderCache.set(cacheKey, next);
    return next;
  }

  async getDefaultAspectRatio(): Promise<number> {
    return this.#firstPageRatio;
  }

  async destroy(): Promise<void> {
    if (this.#destroyed) {
      return;
    }

    this.#destroyed = true;

    await invoke("pdf_close_session", {
      sessionId: this.#sessionId,
    }).catch(() => {
      // Session cleanup is best-effort; the backend worker will drop sessions on app exit.
    });

    this.#renderCache.clear();
  }
}

export async function readInitialPdfPath(): Promise<string | null> {
  return withTimeout(
    invoke<string | null>("initial_pdf_path"),
    FILE_READ_TIMEOUT_MS,
    `Tauri initial_pdf_path timed out after ${FILE_READ_TIMEOUT_MS}ms.`,
  );
}

export async function createPdfSession(path: string): Promise<PdfSession> {
  const sourceMode: PdfSourceMode = "tauri-pdfium";
  const trace = createPdfOpenTrace(path);

  trace.log("open_start", {
    sourceMode,
    renderEngine: "pdfium-render",
  });

  try {
    trace.log("source_prepared", {
      sourceMode,
      path,
    });

    trace.log("getDocument_start", {
      sourceMode,
    });

    const openInfo = await withTimeout(
      invoke<PdfOpenInfo>("pdf_open_info", { path }),
      DOCUMENT_LOAD_TIMEOUT_MS,
      `PDF open timed out after ${DOCUMENT_LOAD_TIMEOUT_MS}ms.`,
    );

    const pageCount = Math.max(0, Math.floor(openInfo.pageCount || 0));
    if (pageCount === 0) {
      throw new Error("PDF has no pages.");
    }

    const firstPageWidth = Math.max(1, openInfo.firstPageWidth || 1);
    const firstPageHeight = Math.max(1, openInfo.firstPageHeight || 1);

    trace.log("getDocument_resolved", {
      sourceMode,
      pageCount,
      renderEngine: openInfo.renderEngine,
    });

    trace.log("first_page_fetch_start", {
      sourceMode,
      pageNumber: MIN_PAGE_NUMBER,
    });

    trace.log("first_page_fetched", {
      sourceMode,
      pageNumber: MIN_PAGE_NUMBER,
      viewport: {
        width: firstPageWidth,
        height: firstPageHeight,
        rotation: 0,
      },
    });

    const diagnostics: PdfSessionDiagnostics = {
      renderEngine: "pdfium-render",
      openAttemptId: trace.openAttemptId,
      openStartedAtMs: trace.openStartedAtMs,
      sourceMode,
    };

    trace.log("open_ready", {
      sourceMode,
      pageCount,
      sessionId: openInfo.sessionId,
      renderEngine: diagnostics.renderEngine,
    });

    return new PdfSession(
      openInfo.sessionId,
      pageCount,
      firstPageHeight / firstPageWidth,
      diagnostics,
    );
  } catch (error) {
    trace.log("open_failed", {
      sourceMode,
      error: describeError(error),
    });

    throw new Error(`Failed to open PDF: "${describeError(error).message}".`);
  }
}
