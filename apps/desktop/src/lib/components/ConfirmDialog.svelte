<script lang="ts">
  import { formatBytes } from "$lib/format";
  import type { CleanupPreview, CleanupProgress } from "$lib/tauri";

  interface Props {
    preview: CleanupPreview;
    measuredAt: Date | null;
    showItems: boolean;
    cleaning: boolean;
    cancelling: boolean;
    progress: CleanupProgress | null;
    onConfirm: () => void;
    onCancel: () => void;
    onCancelCleanup: () => void;
  }

  let { preview, measuredAt, showItems, cleaning, cancelling, progress, onConfirm, onCancel, onCancelCleanup }: Props =
    $props();

  // Explicit understanding gate (G-UI-01). Resets for every new preview.
  let acknowledged = $state(false);
  $effect(() => {
    void preview.total_bytes;
    acknowledged = false;
  });

  const progressPct = $derived(
    progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0,
  );

  function badge(s: string): string {
    return s === "safe"
      ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200"
      : s === "caution"
        ? "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
        : "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200";
  }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" role="dialog" aria-modal="true">
  <div class="w-full max-w-md rounded-xl bg-white p-6 shadow-xl dark:bg-zinc-900">
    <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
      Clean {formatBytes(preview.total_bytes)}?
    </h2>
    <p class="mt-1 text-sm text-zinc-500">
      {preview.items.length} selected · re-measured
      {#if measuredAt}
        {measuredAt.toLocaleTimeString()}
      {/if}
      (scan numbers can go stale)
    </p>

    {#if showItems}
    <ul class="mt-3 max-h-56 space-y-1.5 overflow-auto text-sm">
      {#each preview.items as item (item.resource_id)}
        <li class="rounded-lg border border-zinc-100 p-2 dark:border-zinc-800">
          <div class="flex items-center justify-between gap-2">
            <span class="truncate text-zinc-700 dark:text-zinc-300">✓ {item.name}</span>
            <span class="flex shrink-0 items-center gap-1.5">
              <span class="rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {badge(item.safety)}">
                {item.safety}
              </span>
              <span class="font-mono text-xs text-zinc-500">{formatBytes(item.bytes)}</span>
            </span>
          </div>
          {#if item.warning}
            <p class="mt-1 text-xs text-amber-700 dark:text-amber-300">⚠ {item.warning}</p>
          {/if}
        </li>
      {/each}
    </ul>
    {:else}
      <p class="mt-3 rounded-lg bg-zinc-100 p-2 text-sm text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
        Daftar item disembunyikan (show_preview OFF) — total {formatBytes(preview.total_bytes)} untuk
        {preview.items.length} resource, diukur ulang barusan.
      </p>
    {/if}

    <p class="mt-3 text-xs text-zinc-500">
      These resources can be recreated, but future builds may take longer.
    </p>

    <label class="mt-3 flex items-start gap-2 text-sm text-zinc-700 dark:text-zinc-300">
      <input
        type="checkbox"
        class="mt-0.5 h-4 w-4 accent-emerald-600"
        bind:checked={acknowledged}
        disabled={cleaning}
      />
      Saya paham data ini akan dihapus dan bisa dibuat ulang.
    </label>

    <div class="mt-4 flex justify-end gap-2">
      {#if cleaning}
        <span class="mr-auto self-center text-xs text-zinc-500" role="status">
          {#if progress}
            Cleaning… {progress.done}/{progress.total}
          {:else}
            {cancelling ? "Cancelling…" : "Cleaning…"}
          {/if}
        </span>
      {/if}
      {#if cleaning}
        <button
          class="rounded-lg border border-zinc-200 px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancelCleanup}
          disabled={cancelling}
        >
          {cancelling ? "Cancelling…" : "Cancel remaining"}
        </button>
      {:else}
        <button
          class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancel}
        >
          Cancel
        </button>
      {/if}
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
        onclick={onConfirm}
        disabled={cleaning || !acknowledged}
        title={acknowledged ? "" : "Centang pernyataan pemahaman dulu"}
      >
        {cleaning ? "Cleaning…" : `Clean ${formatBytes(preview.total_bytes)}`}
      </button>
    </div>
    {#if cleaning && progress && progress.total > 0}
      <div class="mt-3 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
        <div class="h-full rounded-full bg-emerald-500" style="width: {progressPct}%"></div>
      </div>
    {/if}
  </div>
</div>
