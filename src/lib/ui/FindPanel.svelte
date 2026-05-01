<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";

  export let pageCount = 0;
  export let currentPage = 1;

  let pageField = String(currentPage);
  let panelElement: HTMLDivElement | null = null;
  let inputElement: HTMLInputElement | null = null;

  const dispatch = createEventDispatcher<{
    close: void;
    jump: { page: number };
  }>();

  export function contains(target: Node): boolean {
    return panelElement?.contains(target) ?? false;
  }

  function submitJump() {
    const parsed = Number.parseInt(pageField, 10);
    if (!Number.isFinite(parsed)) {
      pageField = String(currentPage);
      return;
    }
    dispatch("jump", { page: parsed });
    dispatch("close");
  }

  onMount(() => {
    window.requestAnimationFrame(() => {
      inputElement?.focus();
      inputElement?.select();
    });
  });
</script>

<div
  class="find-panel"
  bind:this={panelElement}
  role="dialog"
  aria-label="Find and jump"
  aria-modal="false"
>
  <div class="find-header">
    <h2>Find</h2>
    <button
      type="button"
      class="find-close"
      on:click={() => dispatch('close')}
      aria-label="Close find panel"
    >
      Close
    </button>
  </div>

  <p class="find-hint">Text search is not available in image mode yet. Jump to a page instead.</p>

  <form class="find-form" on:submit|preventDefault={submitJump}>
    <label class="find-field" for="find-page-input">
      Page
    </label>
    <input
      id="find-page-input"
      bind:this={inputElement}
      bind:value={pageField}
      type="number"
      min="1"
      max={Math.max(1, pageCount)}
      inputmode="numeric"
      aria-label="Jump to page"
    />

    <button type="submit">Go</button>
  </form>
</div>

<style>
  .find-panel {
    position: fixed;
    top: 4.35rem;
    right: 1rem;
    z-index: 40;
    width: min(24rem, calc(100vw - 2rem));
    border: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
    border-radius: 0.8rem;
    background: color-mix(in oklab, var(--panel-raised) 85%, transparent);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    color: var(--text);
    box-shadow: 0 24px 48px rgb(0 0 0 / 0.16), 0 0 0 1px rgb(255 255 255 / 0.1) inset;
    padding: 1rem;
    display: grid;
    gap: 0.75rem;
    animation: find-panel-enter 300ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  @keyframes find-panel-enter {
    from {
      opacity: 0;
      transform: translateY(-12px) scale(0.96) rotateX(8deg);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1) rotateX(0deg);
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
    font-size: 1.05rem;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .find-hint {
    margin: 0;
    color: var(--muted);
    font-size: 0.86rem;
    line-height: 1.4;
  }

  .find-form {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 0.65rem;
    align-items: center;
    margin-top: 0.2rem;
  }

  .find-field {
    font-size: 0.9rem;
    font-weight: 500;
    color: var(--muted);
  }

  .find-form input,
  .find-form button,
  .find-close {
    border: 1px solid var(--line);
    border-radius: 0.6rem;
    background: color-mix(in oklab, var(--panel-raised) 90%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 0.9rem;
    min-height: 2.35rem;
  }

  .find-form input {
    padding: 0 0.7rem;
    font-family: inherit;
  }

  .find-form button,
  .find-close {
    padding: 0 0.85rem;
    font-weight: 500;
    cursor: pointer;
    transition: transform 150ms cubic-bezier(0.16, 1, 0.3, 1), border-color 150ms ease, box-shadow 150ms ease, background-color 150ms ease;
  }

  .find-form button:hover,
  .find-close:hover {
    transform: scale(1.02);
    border-color: color-mix(in oklab, var(--accent) 50%, var(--line));
    background: color-mix(in oklab, var(--line) 40%, var(--panel-raised));
  }

  .find-form button:active,
  .find-close:active {
    transform: scale(0.96);
  }

  .find-form button:focus-visible,
  .find-form input:focus-visible,
  .find-close:focus-visible {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 60%, var(--line));
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 28%, transparent);
  }

  @media (prefers-reduced-motion: reduce) {
    .find-panel {
      animation: none;
    }

    .find-form button,
    .find-close {
      transition: none;
    }

    .find-form button:hover,
    .find-close:hover,
    .find-form button:active,
    .find-close:active {
      transform: none;
    }
  }
</style>
