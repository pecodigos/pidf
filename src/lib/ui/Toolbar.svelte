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
    jump: { page: number };
    zoomin: void;
    zoomout: void;
    zoomreset: void;
    togglesidebar: void;
    toggletheme: void;
  }>();

  let pageField = "1";
  let isEditingPageField = false;

  $: if (!isEditingPageField) {
    pageField = String(currentPage);
  }

  function submitPageJump(): void {
    const parsedPage = Number.parseInt(pageField, 10);
    if (!Number.isFinite(parsedPage)) {
      pageField = String(currentPage);
      return;
    }

    dispatch("jump", { page: parsedPage });
  }

  function handlePageFieldKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      submitPageJump();
    }
  }
</script>

<header class="toolbar">
  <div class="left">
    <button
      class="ghost"
      on:click={() => dispatch("open")}
      disabled={loading}
      title="Open PDF (Ctrl/Cmd+O)"
      aria-label="Open PDF"
    >
      {loading ? "Opening..." : "Open PDF"}
    </button>
    <button
      class="ghost"
      on:click={() => dispatch("togglesidebar")}
      aria-pressed={showSidebar}
      title={showSidebar ? "Hide page rail" : "Show page rail"}
    >
      {showSidebar ? "Hide Page Rail" : "Show Page Rail"}
    </button>
    <span class="file" title={fileName}>{fileName}</span>
  </div>

  <div class="right">
    <button
      class="icon"
      on:click={() => dispatch("zoomout")}
      disabled={loading}
      title="Zoom out (Ctrl/Cmd+-)"
      aria-label="Zoom out"
    >
      -
    </button>
    <button
      class="zoom"
      on:click={() => dispatch("zoomreset")}
      disabled={loading}
      title="Reset zoom (Ctrl/Cmd+0)"
      aria-label="Reset zoom"
    >
      {Math.round(zoom * 100)}%
    </button>
    <button
      class="icon"
      on:click={() => dispatch("zoomin")}
      disabled={loading}
      title="Zoom in (Ctrl/Cmd++)"
      aria-label="Zoom in"
    >
      +
    </button>

    <label class="jump" aria-label="Jump to page">
      <input
        type="text"
        inputmode="numeric"
        aria-label="Page number"
        bind:value={pageField}
        on:focus={() => (isEditingPageField = true)}
        on:blur={() => {
          isEditingPageField = false;
          submitPageJump();
        }}
        on:keydown={handlePageFieldKeydown}
      />
      <span>/ {Math.max(1, pageCount)}</span>
    </label>

    <button
      class="ghost"
      on:click={() => dispatch("toggletheme")}
      aria-pressed={themeDark}
      title={themeDark ? "Switch to light theme" : "Switch to dark theme"}
    >
      Theme: {themeDark ? "Dark" : "Light"}
    </button>
  </div>
</header>

<style>
  .toolbar {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: center;
    padding: 0.6rem 0.9rem;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }

  .left,
  .right {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    min-width: 0;
  }

  .file {
    max-width: 26rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font-size: 0.88rem;
  }

  button,
  input {
    border: 1px solid var(--line);
    background: var(--panel-raised);
    color: var(--text);
    border-radius: 9px;
    height: 2.75rem;
    font: inherit;
  }

  button {
    padding: 0 0.85rem;
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    border-color: color-mix(in oklab, var(--accent) 32%, var(--line));
  }

  button:focus-visible,
  input:focus-visible {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 60%, var(--line));
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 28%, transparent);
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  button[aria-pressed="true"] {
    border-color: color-mix(in oklab, var(--accent) 50%, var(--line));
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent) 34%, transparent);
  }

  .ghost {
    background: var(--panel-raised);
  }

  .icon {
    width: 2.75rem;
    padding: 0;
    font-size: 1.2rem;
  }

  .zoom {
    min-width: 5rem;
  }

  .jump {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    color: var(--muted);
  }

  .jump input {
    width: 4rem;
    text-align: right;
    padding: 0 0.45rem;
  }

  @media (max-width: 860px) {
    .toolbar {
      flex-direction: column;
      align-items: stretch;
    }

    .left,
    .right {
      width: 100%;
      justify-content: flex-start;
      flex-wrap: wrap;
    }

    .file {
      max-width: 100%;
      flex: 1;
    }
  }
</style>
