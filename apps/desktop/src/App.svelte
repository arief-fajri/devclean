<script lang="ts">
  import { onMount } from "svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import ProviderCard from "$lib/components/ProviderCard.svelte";
  import { formatBytes } from "$lib/format";
  import { cleanupStore } from "$lib/stores/cleanup.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";

  let dark = $state(false);

  onMount(() => {
    dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    document.documentElement.classList.toggle("dark", dark);
    scanStore.listen();
    void scanStore.start();
  });

  function toggleDark() {
    dark = !dark;
    document.documentElement.classList.toggle("dark", dark);
  }

  const maxBytes = $derived(
    Math.max(1, ...(scanStore.summary?.providers.map((p) => p.reclaimable_bytes) ?? [1])),
  );
  const failed = $derived((cleanupStore.results ?? []).filter((r) => r.status === "failed"));
  const partial = $derived((cleanupStore.results ?? []).filter((r) => r.status === "partial"));
</script>

<div class="min-h-screen bg-zinc-50 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
  <div class="mx-auto max-w-2xl px-4 pb-32 pt-6">
    <!-- Header -->
    <header class="flex items-center justify-between">
      <h1 class="text-xl font-bold">DevClean</h1>
      <div class="flex items-center gap-2">
        <button
          class="rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
          onclick={toggleDark}
          aria-label="Toggle dark mode"
        >
          {dark ? "☀️" : "🌙"}
        </button>
        <button
          class="rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
          onclick={() => scanStore.start()}
          disabled={scanStore.scanning}
        >
          {scanStore.scanning ? "Scanning…" : "Scan ↻"}
        </button>
      </div>
    </header>

    <!-- Progress -->
    {#if scanStore.scanning}
      <p class="mt-4 text-sm text-zinc-500" role="status">
        {#if scanStore.progress}
          Scanning {scanStore.progress.provider}… ({scanStore.progress.done}/{scanStore.progress.total})
        {:else}
          Starting scan…
        {/if}
      </p>
    {/if}
    {#if scanStore.error}
      <p class="mt-4 rounded-lg bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
        Scan error: {scanStore.error}
      </p>
    {/if}

    <!-- Hero -->
    {#if scanStore.summary}
      <div class="mt-6 text-center">
        <p class="font-mono text-4xl font-bold">{formatBytes(scanStore.summary.reclaimable_bytes)}</p>
        <p class="mt-1 text-sm text-zinc-500">Can be reclaimed</p>
        {#if scanStore.lastScannedAt}
          <p class="mt-1 text-xs text-zinc-400">
            Last scanned {scanStore.lastScannedAt.toLocaleTimeString()}
          </p>
        {/if}
      </div>

      <!-- Provider breakdown -->
      <div class="mt-6 space-y-3">
        {#each scanStore.summary.providers as p (p.provider)}
          <ProviderCard
            summary={p}
            resources={scanStore.resourcesByProvider(p.provider)}
            selected={scanStore.selected}
            maxBytes={maxBytes}
            onToggle={(id) => scanStore.toggle(id)}
            onToggleProvider={(prov) => scanStore.toggleProvider(prov)}
          />
        {/each}
      </div>

      {#if scanStore.summary.providers.length === 0}
        <p class="mt-6 rounded-xl border border-dashed border-zinc-300 p-6 text-center text-sm text-zinc-500 dark:border-zinc-700">
          Nothing reclaimable found. Add a scan root in Settings
          (defaults: ~/Projects, ~/Code, ~/Workspace) and scan again.
        </p>
      {/if}
    {:else if !scanStore.scanning}
      <p class="mt-6 text-center text-sm text-zinc-500">Press Scan to analyze your dev caches.</p>
    {/if}

    <!-- Result banner -->
    {#if cleanupStore.results}
      <div class="mt-6 rounded-xl border border-emerald-200 bg-emerald-50 p-4 dark:border-emerald-900 dark:bg-emerald-950">
        <p class="font-semibold text-emerald-900 dark:text-emerald-100">
          ✓ Cleanup complete — {formatBytes(cleanupStore.freedBytes())} freed
        </p>
        {#if partial.length > 0}
          <p class="mt-1 text-sm text-amber-700 dark:text-amber-300">
            Partially cleaned: {partial.map((r) => r.resource_id).join(", ")}
          </p>
        {/if}
        {#if failed.length > 0}
          <ul class="mt-2 space-y-1 text-sm text-red-700 dark:text-red-300">
            {#each failed as f (f.resource_id)}
              <li>✗ {f.resource_id}: {f.error ?? "unknown error"}</li>
            {/each}
          </ul>
        {/if}
        <button
          class="mt-3 rounded-lg bg-emerald-600 px-4 py-1.5 text-sm font-semibold text-white hover:bg-emerald-700"
          onclick={() => cleanupStore.dismissResults()}
        >
          Done
        </button>
      </div>
    {/if}
    {#if cleanupStore.error}
      <p class="mt-4 rounded-lg bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
        Cleanup error: {cleanupStore.error}
      </p>
    {/if}
  </div>

  <!-- Sticky action bar -->
  {#if scanStore.selected.length > 0}
    <div class="fixed inset-x-0 bottom-0 border-t border-zinc-200 bg-white/90 p-4 backdrop-blur dark:border-zinc-800 dark:bg-zinc-900/90">
      <div class="mx-auto flex max-w-2xl items-center justify-between gap-3">
        <span class="text-sm text-zinc-600 dark:text-zinc-300">
          {scanStore.selected.length} selected · {formatBytes(scanStore.selectedBytes())}
        </span>
        <button
          class="rounded-lg bg-emerald-600 px-5 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
          onclick={() => cleanupStore.openConfirm(scanStore.selected)}
        >
          Clean selected
        </button>
      </div>
    </div>
  {/if}

  <!-- Confirmation (G-UI-01) -->
  {#if cleanupStore.confirmOpen && cleanupStore.preview}
    <ConfirmDialog
      preview={cleanupStore.preview}
      cleaning={cleanupStore.cleaning}
      onConfirm={() => cleanupStore.execute(scanStore.selected)}
      onCancel={() => cleanupStore.close()}
    />
  {/if}
</div>
