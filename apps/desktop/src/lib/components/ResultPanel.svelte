<script lang="ts">
  import { formatBytes } from "$lib/format";
  import type { CleanupResult } from "$lib/tauri";

  interface Props {
    results: CleanupResult[];
    requestedBytes: number;
    freedBytes: number;
    scanning: boolean;
    nameOf: (id: string) => string;
    onRescan: () => void;
    onDone: () => void;
  }

  let { results, requestedBytes, freedBytes, scanning, nameOf, onRescan, onDone }: Props =
    $props();

  const counts = $derived.by(() => {
    let success = 0;
    let partial = 0;
    let failed = 0;
    let cancelled = 0;
    for (const r of results) {
      if (r.status === "success") success += 1;
      else if (r.status === "partial") partial += 1;
      else if (r.status === "failed") failed += 1;
      else cancelled += 1;
    }
    return { success, partial, failed, cancelled };
  });
  const pct = $derived(
    requestedBytes > 0 ? Math.min(100, Math.round((freedBytes / requestedBytes) * 100)) : 0,
  );

  function badge(status: CleanupResult["status"]): string {
    return status === "success"
      ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200"
      : status === "partial"
        ? "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
        : status === "failed"
          ? "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200"
          : "bg-zinc-200 text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300";
  }

  function mark(status: CleanupResult["status"]): string {
    if (status === "success") return "✓";
    if (status === "partial") return "~";
    if (status === "failed") return "✗";
    return "○";
  }
</script>

<div
  class="mt-6 rounded-xl border border-emerald-200 bg-emerald-50 p-4 dark:border-emerald-900 dark:bg-emerald-950"
  role="status"
  aria-label="Cleanup result"
>
  <p class="font-semibold text-emerald-900 dark:text-emerald-100">
    ✓ Cleanup complete —
    <span class="tnum font-mono">{formatBytes(freedBytes)}</span> freed
    <span class="font-normal">(of {formatBytes(requestedBytes)} requested)</span>
  </p>
  <div
    class="mt-2 h-1.5 overflow-hidden rounded-full bg-emerald-100 dark:bg-emerald-900"
    role="img"
    aria-label="{pct}% of requested bytes freed"
  >
    <div class="h-full rounded-full bg-emerald-500" style="width: {pct}%"></div>
  </div>
  <p class="mt-1.5 text-xs text-emerald-800 dark:text-emerald-200">
    {counts.success} success · {counts.partial} partial · {counts.failed} failed ·
    {counts.cancelled} cancelled
  </p>
  <ul class="mt-3 space-y-1.5 text-sm">
    {#each results as r (r.resource_id)}
      <li class="rounded-lg bg-white/70 px-2 py-1.5 dark:bg-zinc-900/60">
        <div class="flex items-center justify-between gap-2">
          <span class="min-w-0 flex-1 truncate" title={nameOf(r.resource_id)}>
            {mark(r.status)}
            {nameOf(r.resource_id)}
          </span>
          <span class="tnum shrink-0 font-mono text-xs text-zinc-500">
            {formatBytes(r.requested_bytes)} → {formatBytes(r.freed_bytes)}
          </span>
          <span
            class="shrink-0 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {badge(
              r.status,
            )}"
          >
            {r.status}
          </span>
        </div>
        {#if r.error}
          <p class="mt-0.5 text-xs text-red-700 dark:text-red-300">{r.error}</p>
          <p class="mt-0.5 text-xs text-zinc-500">
            Next step: retry from the app, or clean it manually and press Rescan to
            re-measure.
          </p>
        {/if}
      </li>
    {/each}
  </ul>
  {#if counts.cancelled > 0}
    <p class="mt-2 text-sm text-zinc-500 dark:text-zinc-400">
      Cancelled: the in-flight item finished, the rest were skipped.
    </p>
  {/if}
  <div class="mt-3 flex gap-2">
    <button
      class="rounded-lg border border-emerald-300 px-4 py-1.5 text-sm text-emerald-800 hover:bg-emerald-100 disabled:opacity-50 dark:border-emerald-800 dark:text-emerald-200 dark:hover:bg-emerald-900"
      onclick={onRescan}
      disabled={scanning}
    >
      Rescan
    </button>
    <button
      class="rounded-lg bg-emerald-600 px-4 py-1.5 text-sm font-semibold text-white hover:bg-emerald-700"
      onclick={onDone}
    >
      Done
    </button>
  </div>
</div>
