<script lang="ts">
  import { formatBytes } from "$lib/format";
  import type { HistoryEntry } from "$lib/tauri";

  interface Props {
    entries: HistoryEntry[];
    loading: boolean;
    error: string | null;
    onRefresh: () => void;
    onClose: () => void;
  }

  let { entries, loading, error, onRefresh, onClose }: Props = $props();

  function when(unix: number): string {
    const d = new Date(unix * 1000);
    return Number.isNaN(d.getTime()) ? "—" : d.toLocaleString();
  }

  function badge(status: HistoryEntry["status"]): string {
    return status === "success"
      ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200"
      : status === "partial"
        ? "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
        : status === "failed"
          ? "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200"
          : "bg-zinc-200 text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300";
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
  role="dialog"
  aria-modal="true"
  aria-label="Cleanup history"
>
  <div class="w-full max-w-md rounded-xl bg-white p-6 shadow-xl dark:bg-zinc-900">
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">Cleanup history</h2>
      <button
        class="rounded-lg border border-zinc-200 px-2 py-1 text-xs hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
        onclick={onRefresh}
        disabled={loading}
      >
        {loading ? "…" : "🔁"}
      </button>
    </div>

    {#if error}
      <p class="mt-3 rounded-lg bg-red-50 p-2 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
        {error}
      </p>
    {:else if entries.length === 0 && !loading}
      <p class="mt-3 text-sm text-zinc-500">No cleanups recorded yet.</p>
    {:else}
      <ul class="mt-3 max-h-72 space-y-1.5 overflow-auto text-sm">
        {#each entries as e (`${e.finished_at_unix}-${e.resource_id}`)}
          <li class="rounded-lg border border-zinc-100 p-2 dark:border-zinc-800">
            <div class="flex items-center justify-between gap-2">
              <span class="min-w-0 flex-1 truncate" title={e.name || e.resource_id}>
                {e.name || e.resource_id}
              </span>
              <span class="shrink-0 font-mono text-xs text-zinc-500">
                {formatBytes(e.requested_bytes)} → {formatBytes(e.freed_bytes)}
              </span>
              <span class="shrink-0 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {badge(e.status)}">
                {e.status}
              </span>
            </div>
            <p class="mt-0.5 text-xs text-zinc-400">
              {when(e.finished_at_unix)}{#if e.provider} · {e.provider}{/if}
            </p>
            {#if e.error}
              <p class="mt-0.5 text-xs text-red-700 dark:text-red-300">{e.error}</p>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    <div class="mt-4 flex justify-end">
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
        onclick={onClose}
      >
        Close
      </button>
    </div>
  </div>
</div>
