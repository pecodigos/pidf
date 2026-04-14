<script lang="ts">
  import { createEventDispatcher } from "svelte";

  export let fileName = "No PDF loaded";
  export let currentPage = 1;
  export let pageCount = 0;
  export let zoom = 1;
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
    <button class="ghost" on:click={() => dispatch("open")} disabled={loading}>
      {loading ? "Opening..." : "Open"}
    </button>
    <button class="ghost" on:click={() => dispatch("togglesidebar")}>
      {showSidebar ? "Pages:On" : "Pages:Off"}
    </button>
    <span class="file" title={fileName}>{fileName}</span>
  </div>

  <div class="right">
    <button class="icon" on:click={() => dispatch("zoomout")} disabled={loading}>-</button>
    <button class="zoom" on:click={() => dispatch("zoomreset")} disabled={loading}>
      {Math.round(zoom * 100)}%
    </button>
    <button class="icon" on:click={() => dispatch("zoomin")} disabled={loading}>+</button>

    <label class="jump" aria-label="Jump to page">
      <input
        type="text"
        inputmode="numeric"
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

    <button class="ghost" on:click={() => dispatch("toggletheme")}>Theme</button>
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
    height: 2rem;
    font: inherit;
  }

  button {
    padding: 0 0.7rem;
    cursor: pointer;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .ghost {
    background: var(--panel-raised);
  }

  .icon {
    width: 2rem;
    padding: 0;
    font-size: 1.05rem;
  }

  .zoom {
    min-width: 4.1rem;
  }

  .jump {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    color: var(--muted);
  }

  .jump input {
    width: 3.3rem;
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
