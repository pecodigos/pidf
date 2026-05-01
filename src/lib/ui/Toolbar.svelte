<script lang="ts">
  import { createEventDispatcher } from "svelte";

  export let fileName = "No PDF loaded";
  export let currentPage = 1;
  export let pageCount = 0;
  export let zoom = 1;
  export let themeDark = false;
  export let loading = false;
  export let showSidebar = false;

  const dispatch = createEventDispatcher<{
    open: void;
    find: void;
    jump: { page: number };
    zoomin: void;
    zoomout: void;
    zoomreset: void;
    togglesidebar: void;
    toggletheme: void;
  }>();

  let pageField = "1";
  let isEditingPageField = false;
  $: normalizedPageCount = Math.max(1, pageCount);

  $: if (!isEditingPageField) {
    pageField = String(currentPage);
  }

  function submitPageJump(): void {
    if (pageCount <= 0) {
      pageField = "1";
      return;
    }

    const parsedPage = Number.parseInt(pageField, 10);
    if (!Number.isFinite(parsedPage)) {
      pageField = String(currentPage);
      return;
    }

    const normalizedPage = Math.max(1, Math.min(normalizedPageCount, parsedPage));
    pageField = String(normalizedPage);
    dispatch("jump", { page: normalizedPage });
  }

  function handlePageFieldKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      submitPageJump();
    }
  }
</script>

<header class="toolbar" role="toolbar" aria-label="PDF controls">
  <div class="toolbar-shell">
    <div class="left-actions">
      <button
        class="btn icon rail"
        on:click={() => dispatch("togglesidebar")}
        aria-pressed={showSidebar}
        title={showSidebar ? "Hide page rail" : "Show page rail"}
        aria-label={showSidebar ? "Hide page rail" : "Show page rail"}
      >
        <span class="rail-glyph" aria-hidden="true"></span>
      </button>
      <button
        class="btn"
        on:click={() => dispatch("open")}
        disabled={loading}
        title="Open PDF (Ctrl/Cmd+O)"
        aria-label="Open PDF"
      >
        {loading ? "Opening..." : "Open"}
      </button>
    </div>

    <div class="title-group" aria-live="polite">
      <p class="file" title={fileName}>{fileName}</p>
      <p class="meta">
        {#if pageCount > 0}
          Page {currentPage} of {normalizedPageCount}
        {:else}
          Open a PDF to begin
        {/if}
      </p>
    </div>

    <div class="right-actions">
      <button
        class="btn"
        on:click={() => dispatch("find")}
        disabled={loading || pageCount <= 0}
        title="Find or jump to page (Ctrl/Cmd+F)"
        aria-label="Find or jump to page"
      >
        Find
      </button>

      <div class="zoom-group" aria-label="Zoom controls">
        <button
          class="btn icon"
          on:click={() => dispatch("zoomout")}
          disabled={loading}
          title="Zoom out (Ctrl/Cmd+-)"
          aria-label="Zoom out"
        >
          -
        </button>
        <button
          class="btn zoom"
          on:click={() => dispatch("zoomreset")}
          disabled={loading}
          title="Reset zoom (Ctrl/Cmd+0)"
          aria-label="Reset zoom"
        >
          {Math.round(zoom * 100)}
        </button>
        <button
          class="btn icon"
          on:click={() => dispatch("zoomin")}
          disabled={loading}
          title="Zoom in (Ctrl/Cmd++)"
          aria-label="Zoom in"
        >
          +
        </button>
      </div>

      <label class="jump" aria-label="Jump to page">
        <span class="jump-label">Page</span>
        <input
          type="number"
          min="1"
          max={normalizedPageCount}
          step="1"
          inputmode="numeric"
          aria-label="Page number"
          bind:value={pageField}
          disabled={loading || pageCount <= 0}
          on:focus={() => (isEditingPageField = true)}
          on:blur={() => {
            isEditingPageField = false;
            submitPageJump();
          }}
          on:keydown={handlePageFieldKeydown}
        />
        <span class="jump-total">/ {normalizedPageCount}</span>
      </label>

      <button
        class="btn theme"
        on:click={() => dispatch("toggletheme")}
        aria-pressed={themeDark}
        title={themeDark ? "Switch to light theme" : "Switch to dark theme"}
      >
        {themeDark ? "Dark" : "Light"}
      </button>
    </div>
  </div>

  {#if loading}
    <div class="loading-track" aria-hidden="true">
      <span class="loading-bar"></span>
    </div>
  {/if}
</header>

<style>
  .toolbar {
    position: sticky;
    top: 0;
    z-index: 24;
    border-bottom: 1px solid var(--line);
    background: color-mix(in oklab, var(--panel) 85%, transparent);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.04), 0 1px 0 rgb(255 255 255 / 0.05) inset;
  }

  .toolbar-shell {
    display: grid;
    grid-template-columns: auto minmax(14rem, 1fr) auto;
    align-items: center;
    gap: 0.65rem;
    padding: 0.52rem 0.75rem;
  }

  .left-actions,
  .right-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
  }

  .right-actions {
    justify-self: end;
  }

  .zoom-group {
    display: inline-flex;
    gap: 0.35rem;
  }

  .title-group {
    min-width: 0;
    display: grid;
    gap: 0.08rem;
    padding: 0 0.32rem;
  }

  .file {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-size: 0.95rem;
    font-weight: 620;
    letter-spacing: 0.01em;
  }

  .meta {
    margin: 0;
    color: var(--muted);
    font-size: 0.72rem;
    letter-spacing: 0.09em;
    text-transform: uppercase;
  }

  button,
  input {
    border: 1px solid var(--line);
    background: var(--panel-raised);
    color: var(--text);
    border-radius: 0.62rem;
    height: 2.32rem;
    font: inherit;
  }

  .btn {
    padding: 0 0.72rem;
    cursor: pointer;
    font-size: 0.84rem;
    letter-spacing: 0.01em;
    transition: transform 150ms cubic-bezier(0.16, 1, 0.3, 1), border-color 150ms ease, box-shadow 150ms ease, background-color 150ms ease;
  }

  .btn.theme {
    min-width: 4.35rem;
  }

  .btn:hover:not(:disabled) {
    transform: scale(1.02);
    border-color: color-mix(in oklab, var(--accent) 40%, var(--line));
    background: var(--panel);
  }

  .btn:active:not(:disabled) {
    transform: scale(0.96);
  }

  .btn:focus-visible,
  input:focus-visible {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 60%, var(--line));
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 28%, transparent);
  }

  .btn:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .btn[aria-pressed="true"] {
    border-color: color-mix(in oklab, var(--accent) 50%, var(--line));
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent) 34%, transparent);
  }

  .icon {
    width: 2.32rem;
    padding: 0;
    font-size: 1.05rem;
  }

  .rail {
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .rail-glyph {
    width: 0.92rem;
    height: 2px;
    border-radius: 2px;
    background: currentColor;
    box-shadow: 0 -5px 0 currentColor, 0 5px 0 currentColor;
  }

  .zoom {
    min-width: 4.6rem;
    font-variant-numeric: tabular-nums;
  }

  .jump {
    display: inline-flex;
    align-items: center;
    gap: 0.28rem;
    min-height: 2.32rem;
    padding: 0 0.46rem;
    border: 1px solid var(--line);
    border-radius: 0.62rem;
    background: var(--panel-raised);
    color: var(--muted);
  }

  .jump:focus-within {
    border-color: color-mix(in oklab, var(--accent) 60%, var(--line));
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 28%, transparent);
  }

  .jump-label {
    font-size: 0.7rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .jump input {
    width: 3.2rem;
    text-align: right;
    padding: 0;
    border: none;
    background: transparent;
    height: auto;
    font-variant-numeric: tabular-nums;
  }

  .jump-total {
    font-size: 0.79rem;
  }

  .loading-track {
    position: relative;
    height: 2px;
    background: color-mix(in oklab, var(--line) 60%, transparent);
    overflow: hidden;
  }

  .loading-bar {
    position: absolute;
    inset: 0 auto 0 0;
    width: 40%;
    background: linear-gradient(90deg, transparent, var(--accent), transparent);
    animation: toolbar-loading 1.1s ease-in-out infinite;
  }

  @keyframes toolbar-loading {
    0% {
      transform: translateX(-120%);
    }

    100% {
      transform: translateX(280%);
    }
  }

  @media (max-width: 860px) {
    .toolbar-shell {
      grid-template-columns: 1fr;
      gap: 0.42rem;
    }

    .left-actions,
    .right-actions {
      width: 100%;
      justify-content: flex-start;
      flex-wrap: wrap;
    }

    .right-actions {
      justify-self: stretch;
    }

    .title-group {
      order: -1;
      padding: 0.1rem 0.2rem;
    }

    .jump {
      margin-left: auto;
    }

    .btn.theme {
      margin-left: auto;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .loading-bar {
      animation: none;
      width: 100%;
      opacity: 0.55;
    }
  }
</style>
