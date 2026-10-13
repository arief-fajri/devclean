<script lang="ts">
  import { formatBytes } from "$lib/format";
  import { Trash2, TriangleAlert } from "lucide-svelte";

  interface Props {
    count: number;
    bytes: number;
    safe: number;
    review: number;
    busy: boolean;
    onClear: () => void;
    onClean: () => void;
  }

  let { count, bytes, safe, review, busy, onClear, onClean }: Props = $props();
</script>

<div
  class="fixed inset-x-0 bottom-0 animate-rise border-t border-zinc-200 bg-white/95 p-4 backdrop-blur motion-reduce:animate-none dark:border-zinc-800 dark:bg-zinc-900/95"
>
  <div class="mx-auto max-w-5xl xl:max-w-7xl">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div class="min-w-0">
        <p class="text-sm font-semibold text-zinc-900 dark:text-zinc-100" aria-live="polite">
          {count} {count === 1 ? "item" : "items"} selected ·
          <span class="tnum font-mono font-normal">{formatBytes(bytes)}</span>
          <span class="font-normal text-zinc-500">reclaimable</span>
        </p>
        <p class="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-zinc-500">
          <span class="inline-flex items-center gap-1">
            <span aria-hidden="true" class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
            {safe} safe
          </span>
          <span class="inline-flex items-center gap-1">
            <span aria-hidden="true" class="h-1.5 w-1.5 rounded-full bg-amber-500"></span>
            {review} need review
          </span>
          <span>No changes made yet</span>
        </p>
      </div>
      <div class="flex items-center gap-2">
        <button
          class="rounded-lg px-3 py-2 text-sm text-zinc-600 hover:bg-zinc-100 disabled:opacity-50 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onClear}
          disabled={busy}
        >
          Clear selection
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
          onclick={onClean}
          disabled={busy}
        >
          <span aria-hidden="true" class="flex"><Trash2 size={15} /></span>
          Review &amp; clean selected
        </button>
      </div>
    </div>
    {#if review > 0}
      <p
        class="mt-2 flex items-center gap-1.5 rounded-lg bg-amber-50 px-2.5 py-1.5 text-xs text-amber-800 dark:bg-amber-950 dark:text-amber-200"
        role="note"
      >
        <span aria-hidden="true" class="flex shrink-0"><TriangleAlert size={13} /></span>
        {review}
        {review === 1 ? "item needs" : "items need"} review — read the consequence before continuing.
        Nothing is deleted until you confirm.
      </p>
    {/if}
  </div>
</div>
