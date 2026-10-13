<script lang="ts">
  import { formatBytes, formatPct } from "$lib/format";
  import { HardDrive, RefreshCw } from "lucide-svelte";

  interface Props {
    reclaimable: number;
    resourceCount: number;
    providerCount: number;
    lastScannedAt: Date | null;
    fromCache: boolean;
    scanning: boolean;
    cancelling: boolean;
    /** e.g. "Scanning Go cache… (3/5)"; null when idle. */
    scanStatus: string | null;
    scanPct: number;
    safeBytes: number;
    safeCategories: number;
    reviewCategories: number;
    onScan: () => void;
    onCancelScan: () => void;
  }

  let {
    reclaimable,
    resourceCount,
    providerCount,
    lastScannedAt,
    fromCache,
    scanning,
    cancelling,
    scanStatus,
    scanPct,
    safeBytes,
    safeCategories,
    reviewCategories,
    onScan,
    onCancelScan,
  }: Props = $props();
</script>

<section
  class="rounded-2xl border border-zinc-200 bg-white p-5 dark:border-zinc-800 dark:bg-zinc-900"
  aria-label="Scan summary"
>
  <div class="flex items-start justify-between gap-4">
    <div class="min-w-0">
      <p class="text-sm text-zinc-500">Potential storage recovery</p>
      <p class="tnum mt-1 text-5xl font-semibold tracking-tight text-zinc-900 dark:text-zinc-50">
        {formatBytes(reclaimable)}
      </p>
      <p class="mt-1.5 text-xs text-zinc-500">
        Across {resourceCount}
        {resourceCount === 1 ? "project" : "projects"} · {providerCount}
        {providerCount === 1 ? "provider" : "providers"}
      </p>
    </div>
    <div
      aria-hidden="true"
      class="hidden h-16 w-16 shrink-0 items-center justify-center rounded-xl bg-emerald-100 sm:flex dark:bg-emerald-950"
    >
      <HardDrive size={28} class="text-emerald-700 dark:text-emerald-300" />
    </div>
  </div>

  <div class="mt-4">
    <div class="flex items-baseline justify-between gap-2 text-xs">
      <span class="text-zinc-500">Safe to clean</span>
      <span class="tnum font-mono text-zinc-700 dark:text-zinc-300">
        {formatBytes(safeBytes)} · {formatPct(safeBytes, reclaimable)}
      </span>
    </div>
    <div
      class="mt-1.5 h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800"
      role="img"
      aria-label="{formatBytes(safeBytes)} of {formatBytes(
        reclaimable,
      )} is safe to clean"
    >
      <div
        class="h-full rounded-full bg-emerald-500"
        style="width: {reclaimable > 0
          ? Math.max(2, Math.min(100, (safeBytes / reclaimable) * 100))
          : 0}%"
      ></div>
    </div>
    <div class="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-xs text-zinc-500">
      <span class="inline-flex items-center gap-1.5">
        <span aria-hidden="true" class="h-2 w-2 rounded-full bg-emerald-500"></span>
        {safeCategories} safe {safeCategories === 1 ? "category" : "categories"}
      </span>
      <span class="inline-flex items-center gap-1.5">
        <span aria-hidden="true" class="h-2 w-2 rounded-full bg-amber-500"></span>
        {reviewCategories} need review
      </span>
    </div>
  </div>

  {#if scanning && scanStatus}
    <p class="mt-3 text-xs text-zinc-500" role="status">{scanStatus}</p>
    <div class="mt-1.5 h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
      <div class="h-full rounded-full bg-emerald-500" style="width: {scanPct}%"></div>
    </div>
  {/if}

  <div
    class="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-zinc-100 pt-4 dark:border-zinc-800"
  >
    <p class="text-xs text-zinc-400">
      {#if lastScannedAt}
        Last scan {lastScannedAt.toLocaleTimeString()}
      {:else}
        Never scanned
      {/if}
      {#if fromCache && !scanning} · cached{/if}
    </p>
    {#if scanning}
      <button
        class="inline-flex items-center gap-2 rounded-lg border border-zinc-200 px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
        onclick={onCancelScan}
        disabled={cancelling}
      >
        {cancelling ? "Cancelling…" : "Cancel scan"}
      </button>
    {:else}
      <button
        class="inline-flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
        onclick={onScan}
      >
        <span aria-hidden="true" class="flex"><RefreshCw size={15} /></span>
        Scan again
      </button>
    {/if}
  </div>
</section>
