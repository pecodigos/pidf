import { invoke } from "@tauri-apps/api/core";

type StageDetails = Record<string, unknown>;

export interface PdfOpenTrace {
  openAttemptId: string;
  path: string;
  openStartedAtMs: number;
  log: (stage: string, details?: StageDetails) => void;
}

const TRACE_STORAGE_KEY = "pidf.trace";

let openAttemptCounter = 0;

function safeJsonStringify(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return "{\"serializationError\":true}";
  }
}

function diagnosticsEnabled(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  try {
    return window.localStorage.getItem(TRACE_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function shouldLogStage(stage: string): boolean {
  if (stage.includes("failed") || stage.includes("error")) {
    return true;
  }

  return diagnosticsEnabled();
}

export function logPdfStage(
  stage: string,
  details: StageDetails = {},
  elapsedMs?: number,
): void {
  if (!shouldLogStage(stage)) {
    return;
  }

  const timestamp = new Date().toISOString();
  const normalizedElapsedMs =
    typeof elapsedMs === "number" && Number.isFinite(elapsedMs)
      ? Math.max(0, Math.round(elapsedMs))
      : null;

  console.info("[PiDF][stage]", {
    stage,
    timestamp,
    ...(normalizedElapsedMs !== null ? { elapsedMs: normalizedElapsedMs } : {}),
    ...details,
  });

  void invoke("trace_pdf_stage", {
    stage,
    timestamp,
    elapsedMs: normalizedElapsedMs,
    detailsJson: safeJsonStringify(details),
  }).catch(() => {
    // Stage relay is best-effort; keep frontend flow independent from tracing IPC.
  });
}

export function createPdfOpenTrace(path: string): PdfOpenTrace {
  openAttemptCounter += 1;

  const openAttemptId = `open-${Date.now()}-${openAttemptCounter}`;
  const openStartedAtMs = Date.now();

  return {
    openAttemptId,
    path,
    openStartedAtMs,
    log(stage, details = {}) {
      logPdfStage(
        stage,
        {
          openAttemptId,
          path,
          ...details,
        },
        Date.now() - openStartedAtMs,
      );
    },
  };
}