<script lang="ts">
  import { createEventDispatcher } from "svelte";

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

<aside class="sidebar" aria-label="Page sidebar">
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
      >
        {page}
      </button>
    {/each}
  </div>
</aside>

<style>
  .sidebar {
    width: 6.2rem;
    border-right: 1px solid var(--line);
    background: var(--panel);
    display: grid;
    grid-template-rows: auto 1fr;
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
  }

  button:hover {
    border-color: color-mix(in oklab, var(--accent) 32%, var(--line));
  }

  button:focus-visible {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 60%, var(--line));
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 28%, transparent);
  }

  button.selected {
    color: var(--text);
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent) 40%, transparent);
  }
</style>
