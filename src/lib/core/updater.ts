import { check } from "@tauri-apps/plugin-updater";
import type { Update } from "@tauri-apps/plugin-updater";

export interface UpdateStatus {
  available: boolean;
  latestVersion?: string;
  message: string;
}

let updateCheckInProgress = false;

/**
 * Check for updates. Returns status regardless of result.
 * Does not install — call installUpdate() separately.
 */
export async function checkForUpdate(): Promise<UpdateStatus> {
  if (updateCheckInProgress) {
    return {
      available: false,
      message: "Update check already in progress.",
    };
  }

  updateCheckInProgress = true;

  try {
    const update: Update | null = await check();

    if (update) {
      return {
        available: true,
        latestVersion: update.version,
        message: `v${update.version} available.`,
      };
    }

    return {
      available: false,
      message: "PiDF is up to date.",
    };
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    return {
      available: false,
      message: `Update check failed: ${msg}`,
    };
  } finally {
    updateCheckInProgress = false;
  }
}

/**
 * Download and install an update.
 * Must call checkForUpdate() first — this function checks again internally.
 */
export async function installUpdate(): Promise<string> {
  try {
    const update: Update | null = await check();

    if (!update) {
      return "No update available.";
    }

    await update.downloadAndInstall((_event) => {
      // Progress tracking could be added here in the future.
    });

    return `Update to v${update.version} installed. Restart PiDF to apply.`;
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    throw new Error(`Update install failed: ${msg}`);
  }
}
