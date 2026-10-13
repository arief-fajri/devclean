<script lang="ts">
  import { viewStore } from "$lib/stores/view.svelte";
  import { Search, X, Zap } from "lucide-svelte";

  interface Props {
    totalCount: number;
    safeCount: number;
    reviewCount: number;
    staleCount: number;
    selectedCount: number;
    resultCount: number;
    /** Number of visible-but-unselected safe items (0 hides the button). */
    selectableSafe: number;
    onSelectSafe: () => void;
  }

  let {
    totalCount,
    safeCount,
    reviewCount,
    staleCount,
    selectedCount,
    resultCount,
    selectableSafe,
    onSelectSafe,
  }: Props = $props();

  let searchEl = $state<HTMLInputElement | null>(null);

  const chip = (active: boolean) =>
    `rounded-full border px-2.5 py-1 text-xs font-medium ${
      active
        ? "border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900"
        : "border-zinc-200 text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
    }`;

  function onWindowKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    const typing =
      t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT");
    if (e.key === "/" && !typing) {
      e.preventDefault();
      searchEl?.focus();
    }
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<section
  class="rounded-xl border border-zinc-200 bg-white p-3 dark:border-zinc-800 dark:bg-zinc-900"
  aria-label="Search and filter results"
>
  <!-- Row 1: search + sort + select-safe (all h-8 via .control) -->
  <div class="flex items-center gap-2">
    <span aria-hidden="true" class="flex shrink-0 text-zinc-400">
      <Search size={16} />
    </span>
    <input
      bind:this={searchEl}
      type="search"
      class="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-zinc-400"
      placeholder="Search caches, projects…  ( / )"
      bind:value={viewStore.query}
      onkeydown={(e) => {
        if (e.key === "Escape") viewStore.query = "";
      }}
      aria-label="Search caches and projects"
    />
    {#if viewStore.query}
      <button
        class="flex shrink-0 rounded p-0.5 text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200"
        onclick={() => {
          viewStore.query = "";
          searchEl?.focus();
        }}
        aria-label="Clear search"
      >
        <span aria-hidden="true" class="flex"><X size={14} /></span>
      </button>
    {/if}
    <label class="flex shrink-0 items-center">
      <span class="sr-only">Sort results</span>
      <select
        class="control"
        bind:value={viewStore.sort}
        aria-label="Sort results"
      >
        <option value="largest">Largest first</option>
        <option value="safety">Needs review first</option>
        <option value="name">Name</option>
      </select>
    </label>
    {#if selectableSafe > 0}
      <button
        class="control shrink-0 border-emerald-300 font-semibold text-emerald-700 hover:bg-emerald-50 dark:border-emerald-800 dark:text-emerald-300 dark:hover:bg-emerald-950"
        onclick={onSelectSafe}
        title="Select all safe items in the current view"
      >
        <span aria-hidden="true" class="flex"><Zap size={12} /></span>
        Select safe ({selectableSafe})
      </button>
    {/if}
  </div>

  <!-- Row 2: filters + result count + paths toggle -->
  <div class="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-2">
    <div class="flex min-w-0 flex-1 flex-wrap items-center gap-1.5" role="group" aria-label="Safety filters">
      <button class={chip(viewStore.safety === "all" && !viewStore.staleOnly && !viewStore.selectedOnly)} onclick={() => { viewStore.safety = "all"; viewStore.staleOnly = false; viewStore.selectedOnly = false; }} aria-pressed={viewStore.safety === "all" && !viewStore.staleOnly && !viewStore.selectedOnly}>
        All · {totalCount}
      </button>
      <button class={chip(viewStore.safety === "safe")} onclick={() => { viewStore.safety = viewStore.safety === "safe" ? "all" : "safe"; }} aria-pressed={viewStore.safety === "safe"}>
        Safe · {safeCount}
      </button>
      <button class={chip(viewStore.safety === "review")} onclick={() => { viewStore.safety = viewStore.safety === "review" ? "all" : "review"; }} aria-pressed={viewStore.safety === "review"}>
        Review · {reviewCount}
      </button>
      <button class={chip(viewStore.staleOnly)} onclick={() => { viewStore.staleOnly = !viewStore.staleOnly; }} aria-pressed={viewStore.staleOnly} title="Untouched for >90 days — a signal only, never a safety verdict">
        Stale · {staleCount}
      </button>
      <button class={chip(viewStore.selectedOnly)} onclick={() => { viewStore.selectedOnly = !viewStore.selectedOnly; }} aria-pressed={viewStore.selectedOnly}>
        Selected · {selectedCount}
      </button>
    </div>
    <p class="shrink-0 text-xs text-zinc-500" role="status">
      {resultCount} {resultCount === 1 ? "result" : "results"}
      {#if viewStore.active}
        <button class="ml-1 underline hover:text-zinc-700 dark:hover:text-zinc-300" onclick={() => viewStore.reset()}>
          clear
        </button>
      {/if}
    </p>
    <label
      class="flex shrink-0 cursor-pointer items-center gap-1.5 text-xs text-zinc-500"
      title="Show the filesystem path under each item"
    >
      <input type="checkbox" class="h-3.5 w-3.5 accent-emerald-600" bind:checked={viewStore.showPaths} />
      Paths
    </label>
  </div>
</section>
