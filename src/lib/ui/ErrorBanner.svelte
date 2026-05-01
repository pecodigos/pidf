<script lang="ts">
  import { createEventDispatcher } from "svelte";

  export let message = "";
  export let hasDetails = false;
  export let copyFeedback = "";
  export let loading = false;
  export let canRetry = false;

  const dispatch = createEventDispatcher<{
    retry: void;
    open: void;
    copydetails: void;
  }>();
</script>

<div class="error-banner" role="alert">
  <p>{message}</p>
  <div class="actions">
    <button on:click={() => dispatch('retry')} disabled={loading || !canRetry}>Retry</button>
    <button on:click={() => dispatch('open')} disabled={loading}>Open Another PDF</button>
    <button on:click={() => dispatch('copydetails')} disabled={!hasDetails}>Copy Details</button>
    {#if copyFeedback}
      <span class="feedback" role="status" aria-live="polite">{copyFeedback}</span>
    {/if}
  </div>
</div>

<style>
  .error-banner {
    position: absolute;
    bottom: 1.5rem;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1.25rem;
    padding: 0.85rem 1.25rem;
    border-radius: 0.8rem;
    background: color-mix(in oklab, #fee2e2 92%, transparent);
    border: 1px solid color-mix(in oklab, #f87171 40%, transparent);
    color: #7f1d1d;
    box-shadow: 0 16px 32px rgb(0 0 0 / 0.12), 0 0 0 1px rgb(255 255 255 / 0.4) inset;
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    z-index: 50;
    max-width: calc(100vw - 2rem);
    animation: error-in 0.4s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  :global(.app.dark) .error-banner {
    background: color-mix(in oklab, #1e0909 85%, transparent);
    border-color: color-mix(in oklab, #991b1b 60%, transparent);
    box-shadow: 0 16px 32px rgb(0 0 0 / 0.4), 0 0 0 1px rgb(255 255 255 / 0.05) inset;
    color: #fecaca;
  }

  @keyframes error-in {
    from {
      opacity: 0;
      transform: translate(-50%, 16px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0) scale(1);
    }
  }

  p {
    margin: 0;
    font-size: 0.9rem;
    font-weight: 500;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  button {
    border: 1px solid color-mix(in oklab, currentColor 20%, transparent);
    border-radius: 0.5rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
    color: inherit;
    font: inherit;
    font-size: 0.8rem;
    font-weight: 600;
    padding: 0.4rem 0.8rem;
    cursor: pointer;
    transition: transform 150ms cubic-bezier(0.16, 1, 0.3, 1), background-color 150ms ease, box-shadow 150ms ease;
  }

  button:hover:not(:disabled) {
    transform: scale(1.02);
    background: color-mix(in oklab, currentColor 12%, transparent);
  }

  button:active:not(:disabled) {
    transform: scale(0.96);
  }

  button:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px color-mix(in oklab, currentColor 40%, transparent);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .feedback {
    font-size: 0.75rem;
    font-weight: 600;
    opacity: 0.85;
    margin-left: 0.25rem;
    animation: fade-in 0.2s ease forwards;
  }

  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 0.85; }
  }

  @media (prefers-reduced-motion: reduce) {
    .error-banner,
    .feedback {
      animation: none;
    }

    button {
      transition: none;
    }

    button:hover:not(:disabled),
    button:active:not(:disabled) {
      transform: none;
    }
  }
</style>
