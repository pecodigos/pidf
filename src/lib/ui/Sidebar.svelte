<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { slide } from "svelte/transition";
  import UpdateChecker from "$lib/ui/UpdateChecker.svelte";

  export let pageCount = 0;
  export let currentPage = 1;

  const dispatch = createEventDispatcher<{ jump: { page: number } }>();

  function buildPageWindow(center: number, totalPages: number): number[] {
    if (totalPages <= 0) {
      return [];
    }

    const maxButtons = 40;
    const half = Math.floor(maxButtons / 2);
    const start = Math.max(1, center - half);
    const end = Math.min(totalPages, start + maxButtons - 1);
    const normalizedStart = Math.max(1, end - maxButtons + 1);

    const pages: number[] = [];
    for (let page = normalizedStart; page <= end; page += 1) {
      pages.push(page);
    }

    return pages;
  }

  $: visiblePages = buildPageWindow(currentPage, pageCount);
  $: firstVisiblePage = visiblePages.length > 0 ? visiblePages[0] : 1;
  $: lastVisiblePage = visiblePages.length > 0 ? visiblePages[visiblePages.length - 1] : 1;
</script>

<aside class="sidebar" aria-label="Page sidebar" transition:slide={{ axis: 'x', duration: 250 }}>
  <div class="meta">
    <p>{pageCount} pages</p>
    {#if pageCount > 0}<p>{firstVisiblePage}-{lastVisiblePage}</p>{/if}
  </div>

  <div class="pages">
    {#each visiblePages as page}
      <button
        class:selected={page === currentPage}
        on:click={() => dispatch("jump", { page })}
        aria-label={`Go to page ${page}`}
        aria-current={page === currentPage ? "page" : undefined}
      >
        {page}
      </button>
    {/each}
  </div>

  <UpdateChecker />
</aside>

<style>
  .sidebar {
    width: 6.2rem;
    border-right: 1px solid var(--line);
    background: var(--panel);
    display: grid;
    grid-template-rows: auto 1fr auto;
    min-height: 0;
  }

  .meta {
    border-bottom: 1px solid var(--line);
    padding: 0.6rem;
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.25;
  }

  .meta p {
    margin: 0;
  }

  .pages {
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    padding: 0.55rem;
  }

  button {
    min-height: 2.75rem;
    border: 1px solid var(--line);
    border-radius: 0.55rem;
    background: var(--panel-raised);
    color: var(--muted);
    cursor: pointer;
    font-size: 0.86rem;
    transition: transform 150ms cubic-bezier(0.16, 1, 0.3, 1), border-color 150ms ease, box-shadow 150ms ease, background-color 150ms ease;
  }

  button:hover {
    transform: scale(1.02);
    border-color: color-mix(in oklab, var(--accent) 40%, var(--line));
    background: var(--panel);
  }

  button:active {
    transform: scale(0.96);
  }

  button:focus-visible {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 60%, var(--line));
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 28%, transparent);
  }

  button.selected {
    color: var(--text);
    border-color: var(--accent);
    background: color-mix(in oklab, var(--accent) 10%, var(--panel-raised));
    box-shadow: 0 4px 12px rgb(0 0 0 / 0.1), inset 0 0 0 1px color-mix(in oklab, var(--accent) 40%, transparent);
  }

  @media (prefers-reduced-motion: reduce) {
    button {
      transition: none;
    }

    button:hover,
    button:active {
      transform: none;
    }
  }
</style>
