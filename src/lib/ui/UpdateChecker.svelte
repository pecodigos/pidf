<script lang="ts">
  import { checkForUpdate, installUpdate, type UpdateStatus } from "$lib/core/updater";

  let status = "";
  let checking = false;
  let installing = false;
  let updateAvailable = false;

  async function handleCheck(): Promise<void> {
    if (checking || installing) return;

    checking = true;
    status = "Checking...";
    updateAvailable = false;

    try {
      const result: UpdateStatus = await checkForUpdate();
      status = result.message;
      updateAvailable = result.available;
    } catch (error) {
      status = error instanceof Error ? error.message : "Check failed.";
    } finally {
      checking = false;
    }
  }

  async function handleInstall(): Promise<void> {
    if (!updateAvailable || installing) return;

    installing = true;
    status = "Downloading and installing...";

    try {
      const msg = await installUpdate();
      status = msg;
      updateAvailable = false;
    } catch (error) {
      status = error instanceof Error ? error.message : "Install failed.";
    } finally {
      installing = false;
    }
  }
</script>

<div class="updater">
  <button
    class="check-btn"
    on:click={handleCheck}
    disabled={checking || installing}
    aria-label="Check for updates"
  >
    {checking ? "Checking..." : installing ? "Installing..." : "Check for Updates"}
  </button>

  {#if updateAvailable}
    <button
      class="install-btn"
      on:click={handleInstall}
      disabled={installing}
      aria-label="Install update"
    >
      Install Update
    </button>
  {/if}

  {#if status}
    <p class="status" aria-live="polite">{status}</p>
  {/if}
</div>

<style>
  .updater {
    border-top: 1px solid var(--line);
    padding: 0.55rem;
    display: grid;
    gap: 0.35rem;
  }

  .check-btn,
  .install-btn {
    min-height: 2.2rem;
    border: 1px solid var(--line);
    border-radius: 0.55rem;
    background: var(--panel-raised);
    color: var(--muted);
    cursor: pointer;
    font: inherit;
    font-size: 0.78rem;
    letter-spacing: 0.01em;
    transition: border-color 150ms ease, background-color 150ms ease;
  }

  .check-btn:hover:not(:disabled),
  .install-btn:hover:not(:disabled) {
    border-color: color-mix(in oklab, var(--accent) 40%, var(--line));
    background: var(--panel);
  }

  .check-btn:disabled,
  .install-btn:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .install-btn {
    color: var(--text);
    border-color: var(--accent);
    background: color-mix(in oklab, var(--accent) 12%, var(--panel-raised));
  }

  .status {
    margin: 0;
    font-size: 0.68rem;
    color: var(--muted);
    line-height: 1.35;
    word-break: break-word;
  }
</style>
