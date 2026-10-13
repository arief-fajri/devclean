<script lang="ts">
  import { formatBytes } from "$lib/format";
  import type { CleanupPreview } from "$lib/tauri";

  interface Props {
    preview: CleanupPreview;
    cleaning: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let { preview, cleaning, onConfirm, onCancel }: Props = $props();
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" role="dialog" aria-modal="true">
  <div class="w-full max-w-md rounded-xl bg-white p-6 shadow-xl dark:bg-zinc-900">
    <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
      Clean {formatBytes(preview.total_bytes)}?
    </h2>
    <p class="mt-1 text-sm text-zinc-500">You selected:</p>

    <ul class="mt-3 max-h-56 space-y-1.5 overflow-auto text-sm">
      {#each preview.items as item (item.resource_id)}
        <li class="flex items-center justify-between gap-2">
          <span class="truncate text-zinc-700 dark:text-zinc-300">✓ {item.name}</span>
          <span class="shrink-0 font-mono text-xs text-zinc-500">{formatBytes(item.bytes)}</span>
        </li>
      {/each}
    </ul>

    {#if preview.items.some((i) => i.warning)}
      <p class="mt-3 rounded-lg bg-amber-50 p-2 text-xs text-amber-800 dark:bg-amber-950 dark:text-amber-200">
        {preview.items.find((i) => i.warning)?.warning}
        These resources can be recreated, but future builds may take longer.
      </p>
    {:else}
      <p class="mt-3 text-xs text-zinc-500">
        These resources can be recreated, but future builds may take longer.
      </p>
    {/if}

    <div class="mt-5 flex justify-end gap-2">
      <button
        class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
        onclick={onCancel}
        disabled={cleaning}
      >
        Cancel
      </button>
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
        onclick={onConfirm}
        disabled={cleaning}
      >
        {cleaning ? "Cleaning…" : `Clean ${formatBytes(preview.total_bytes)}`}
      </button>
    </div>
  </div>
</div>
