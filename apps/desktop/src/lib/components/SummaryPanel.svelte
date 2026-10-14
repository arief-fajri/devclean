<script lang="ts">
  import { formatBytes, formatPct } from "$lib/format";
  import { providerMeta } from "$lib/providerMeta";
  import { HardDrive, LoaderCircle, RefreshCw, TriangleAlert } from "lucide-svelte";

  interface Props {
    reclaimable: number;
    resourceCount: number;
    providerCount: number;
    lastScannedAt: Date | null;
    fromCache: boolean;
    /** Providers refreshed by the last targeted re-scan; null = full scan. */
    partialScan: string[] | null;
    scanning: boolean;
    cancelling: boolean;
    /** e.g. "Scanning Go cache… (3/9)"; null when idle. */
    scanStatus: string | null;
    scanPct: number;
    /** Provider currently running (v0.5.0), or null. */
    activeProvider: string | null;
    /** Providers finished this run, in completion order. */
    doneProviders: string[];
    /** How many providers this run will cover. */
    totalProviders: number;
    safeBytes: number;
    safeCategories: number;
    reviewCategories: number;
    /** Bytes held back by the retention policy. */
    retentionBlocked: number;
    retentionDays: number | null;
    onScan: () => void;
    onCancelScan: () => void;
  }

  let {
    reclaimable,
    resourceCount,
    providerCount,
    lastScannedAt,
    fromCache,
    partialScan,
    scanning,
    cancelling,
    scanStatus,
    scanPct,
    activeProvider,
    doneProviders,
    totalProviders,
    safeBytes,
    safeCategories,
    reviewCategories,
    retentionBlocked,
    retentionDays,
    onScan,
    onCancelScan,
  }: Props = $props();

  const activeLabel = $derived(
    activeProvider ? (providerMeta(activeProvider).label ?? activeProvider) : null,
  );
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
        {#if scanning}
          <span
            class="ml-1.5 rounded bg-amber-100 px-1.5 py-0.5 font-medium text-amber-800 dark:bg-amber-950/60 dark:text-amber-200"
            >from the last scan — numbers may change</span
          >
        {/if}
      </p>
    </div>
    <div
      aria-hidden="true"
      class="hidden h-16 w-16 shrink-0 items-center justify-center rounded-xl bg-emerald-100 sm:flex dark:bg-emerald-950"
    >
      <HardDrive size={28} class="text-emerald-700 dark:text-emerald-300" />
    </div>
  </div>

  {#if retentionBlocked > 0}
    <div
      class="mt-4 flex flex-wrap items-center gap-2 rounded-lg bg-zinc-100 px-3 py-2 text-xs dark:bg-zinc-800"
      data-testid="retention-blocked"
    >
      <span class="font-medium text-zinc-700 dark:text-zinc-300">
        {formatBytes(retentionBlocked)} held by your {retentionDays ?? 0}-day retention
      </span>
      <span class="text-zinc-500">
        — those items are shown but cannot be selected for cleanup.
      </span>
    </div>
  {/if}

  <!-- While scanning, progress REPLACES the safe/review breakdown: the old
       numbers are the point of the scan, so showing a confident bar over
       them is misleading. -->
  {#if scanning}
    <div class="mt-5" role="status" aria-live="polite">
      <div class="flex items-baseline justify-between gap-2 text-sm">
        <span class="inline-flex items-center gap-2 font-medium text-zinc-700 dark:text-zinc-300">
          {#if activeProvider}
            <span aria-hidden="true" class="flex animate-spin text-emerald-600"
              ><LoaderCircle size={15} /></span
            >
          {:else}
            <span aria-hidden="true" class="flex animate-spin text-zinc-400"
              ><LoaderCircle size={15} /></span
            >
          {/if}
          {scanStatus ?? "Starting scan…"}
        </span>
        <span class="tnum font-mono text-zinc-600 dark:text-zinc-300">{scanPct}%</span>
      </div>

      <div
        class="mt-2 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800"
        role="progressbar"
        aria-valuenow={scanPct}
        aria-valuemin="0"
        aria-valuemax="100"
        aria-label="Scan progress"
      >
        <div
          class="h-full rounded-full bg-emerald-500 transition-[width] duration-300 motion-reduce:transition-none"
          style="width: {scanPct}%"
        ></div>
      </div>

      {#if totalProviders > 0}
        <ul class="mt-3 flex flex-wrap gap-x-3 gap-y-1 text-xs" aria-label="Providers scanned">
          {#each doneProviders as pid (pid)}
            {@const meta = providerMeta(pid)}
            <li class="inline-flex items-center gap-1.5 text-emerald-700 dark:text-emerald-400">
              <span aria-hidden="true" class="flex"><meta.tick size={12} strokeWidth={2.5} /></span>
              {meta.label ?? pid}
            </li>
          {/each}
          {#if activeLabel}
            <li class="inline-flex items-center gap-1.5 text-zinc-600 dark:text-zinc-300">
              <span aria-hidden="true" class="flex animate-spin"
                ><LoaderCircle size={12} /></span
              >
              {activeLabel}
            </li>
          {/if}
        </ul>
      {/if}
    </div>
  {:else}
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
    {#if partialScan && !scanning}
      <p
        class="mt-2 flex items-start gap-1.5 rounded-lg bg-amber-50 px-2.5 py-1.5 text-[11px] text-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
        data-testid="partial-scan-note"
      >
        <span aria-hidden="true" class="mt-0.5 flex shrink-0"><TriangleAlert size={12} /></span>
        <span>
          Partial re-scan — {partialScan.length}
          {partialScan.length === 1 ? "provider" : "providers"} refreshed
          {partialScan.length < totalProviders ? `of ${totalProviders}` : ''}. The other
          providers keep their last known sizes and may already have drifted.
        </span>
      </p>
    {/if}
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