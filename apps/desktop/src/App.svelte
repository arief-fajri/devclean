<script lang="ts">
  import { onMount } from "svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import HistoryDialog from "$lib/components/HistoryDialog.svelte";
  import ProviderCard from "$lib/components/ProviderCard.svelte";
  import ResourceDetail from "$lib/components/ResourceDetail.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import { formatBytes } from "$lib/format";
  import { cleanupStore } from "$lib/stores/cleanup.svelte";
  import { historyStore } from "$lib/stores/history.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";
  import { settingsStore } from "$lib/stores/settings.svelte";

  let dark = $state(false);

  function applyTheme(theme: string | undefined) {
    const sysDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    dark = theme === "dark" || (theme !== "light" && sysDark);
    document.documentElement.classList.toggle("dark", dark);
  }

  onMount(() => {
    scanStore.listen();
    cleanupStore.listen();
    // Settings first (persisted theme), then instant cache paint,
    // then an honest background rescan.
    void (async () => {
      await settingsStore.load();
      applyTheme(settingsStore.settings?.theme);
      await scanStore.loadCached();
      await scanStore.start();
    })();
  });

  /** Clean entry point: confirmation dialog by default (G-UI-01).
   * Only skipped when the user explicitly disabled require_confirmation. */
  function handleClean() {
    const s = settingsStore.settings;
    if (s && !s.require_confirmation) {
      void cleanupStore.execute(scanStore.selected);
    } else {
      void cleanupStore.openConfirm(scanStore.selected);
    }
  }

  async function handleSaveSettings(next: Parameters<typeof settingsStore.save>[0]) {
    const shouldRescan = await settingsStore.save(next);
    applyTheme(settingsStore.settings?.theme);
    if (shouldRescan) await scanStore.start();
  }

  function toggleDark() {
    const next: "light" | "dark" = dark ? "light" : "dark";
    applyTheme(next);
    void settingsStore.setTheme(next);
  }

  const maxBytes = $derived(
    Math.max(1, ...(scanStore.summary?.providers.map((p) => p.reclaimable_bytes) ?? [1])),
  );
  const cancelled = $derived(
    (cleanupStore.results ?? []).filter((r) => r.status === "cancelled"),
  );
  const scanPct = $derived(
    scanStore.progress && scanStore.progress.total > 0
      ? Math.round((scanStore.progress.done / scanStore.progress.total) * 100)
      : 0,
  );
  const safetyCounts = $derived.by(() => {
    const map = new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r]));
    let safe = 0;
    let caution = 0;
    for (const id of scanStore.selected) {
      const r = map.get(id);
      if (r?.safety === "caution") caution += 1;
      else if (r) safe += 1;
    }
    return { safe, caution };
  });
</script>

<div class="min-h-screen bg-zinc-50 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
  <div class="mx-auto max-w-2xl px-4 pb-32 pt-6">
    <!-- Header -->
    <header class="flex items-center justify-between">
      <h1 class="text-xl font-bold">DevClean</h1>
      <div class="flex items-center gap-2">
        <button
          class="rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
          onclick={() => historyStore.openDialog()}
          aria-label="Open cleanup history"
          title="Cleanup history"
        >
          🕘
        </button>
        <button
          class="rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
          onclick={() => settingsStore.openDialog()}
          aria-label="Open settings"
          title="Settings"
        >
          ⚙
        </button>
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
        {#if scanStore.scanning}
          <button
            class="rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
            onclick={() => scanStore.cancel()}
            disabled={scanStore.cancelling}
          >
            {scanStore.cancelling ? "Cancelling…" : "Cancel ✕"}
          </button>
        {/if}
      </div>
    </header>

    <!-- Progress -->
    {#if scanStore.scanning}
      <div class="mt-4" role="status">
        <p class="text-sm text-zinc-500">
          {#if scanStore.progress}
            Scanning {scanStore.progress.provider}… ({scanStore.progress.done}/{scanStore.progress
              .total})
          {:else}
            {scanStore.cancelling ? "Cancelling scan…" : "Starting scan…"}
          {/if}
        </p>
        {#if scanStore.progress && scanStore.progress.total > 0}
          <div class="mt-2 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
            <div class="h-full rounded-full bg-emerald-500" style="width: {scanPct}%"></div>
          </div>
        {/if}
      </div>
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
        <p class="mt-1 text-xs text-zinc-400">
          {scanStore.summary.resources.length} projects · {scanStore.summary.providers.length} providers
          {#if scanStore.lastScannedAt}
            · Last scanned {scanStore.lastScannedAt.toLocaleTimeString()}
          {/if}
          {#if scanStore.fromCache && !scanStore.scanning}
            · cached
          {/if}
        </p>
      </div>

      <!-- Provider breakdown -->
      <div class="mt-6 space-y-3">
        {#each scanStore.summary.providers as p (p.provider)}
          <ProviderCard
            summary={p}
            resources={scanStore.resourcesByProvider(p.provider)}
            selected={scanStore.selected}
            maxBytes={maxBytes}
            expanded={scanStore.expanded.includes(p.provider)}
            rescanning={scanStore.rescanning === p.provider}
            onToggle={(id) => scanStore.toggle(id)}
            onToggleProvider={(prov) => scanStore.toggleProvider(prov)}
            onExpand={(prov) => scanStore.toggleExpand(prov)}
            onRescan={(prov) => scanStore.rescanProvider(prov)}
            onOpenDetail={(id) => scanStore.openDetail(id)}
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

    <!-- Result: per-resource before/after (I8, G-REL-02) -->
    {#if cleanupStore.results}
      <div class="mt-6 rounded-xl border border-emerald-200 bg-emerald-50 p-4 dark:border-emerald-900 dark:bg-emerald-950">
        <p class="font-semibold text-emerald-900 dark:text-emerald-100">
          ✓ Cleanup complete — {formatBytes(cleanupStore.freedBytes())} freed
          <span class="font-normal">(of {formatBytes(cleanupStore.requestedBytes())} requested)</span>
        </p>
        <ul class="mt-3 space-y-1.5 text-sm">
          {#each cleanupStore.results as r (r.resource_id)}
            <li
              class="flex items-center justify-between gap-2 rounded-lg bg-white/70 px-2 py-1.5 dark:bg-zinc-900/60"
            >
              <span class="min-w-0 flex-1 truncate" title={cleanupStore.nameOf(r.resource_id)}>
                {#if r.status === "success"}✓{:else if r.status === "partial"}~{:else if r.status === "failed"}✗{:else}○{/if}
                {cleanupStore.nameOf(r.resource_id)}
              </span>
              <span class="shrink-0 font-mono text-xs text-zinc-500">
                {formatBytes(r.requested_bytes)} → {formatBytes(r.freed_bytes)}
              </span>
              <span
                class="shrink-0 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {r.status ===
                'success'
                  ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200'
                  : r.status === 'partial'
                    ? 'bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200'
                    : r.status === 'failed'
                      ? 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
                      : 'bg-zinc-200 text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300'}"
              >
                {r.status}
              </span>
            </li>
            {#if r.error}
              <li class="-mt-1 pl-2 text-xs text-red-700 dark:text-red-300">{r.error}</li>
            {/if}
          {/each}
        </ul>
        {#if cancelled.length > 0}
          <p class="mt-2 text-sm text-zinc-500 dark:text-zinc-400">
            Cancelled: the in-flight item finished, the rest were skipped.
          </p>
        {/if}
        <div class="mt-3 flex gap-2">
          <button
            class="rounded-lg border border-emerald-300 px-4 py-1.5 text-sm text-emerald-800 hover:bg-emerald-100 dark:border-emerald-800 dark:text-emerald-200 dark:hover:bg-emerald-900"
            onclick={() => scanStore.start()}
            disabled={scanStore.scanning}
          >
            Rescan
          </button>
          <button
            class="rounded-lg bg-emerald-600 px-4 py-1.5 text-sm font-semibold text-white hover:bg-emerald-700"
            onclick={() => cleanupStore.dismissResults()}
          >
            Done
          </button>
        </div>
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
          {#if safetyCounts.caution > 0}
            <span class="text-amber-600 dark:text-amber-400">
              (SAFE {safetyCounts.safe} · CAUTION {safetyCounts.caution})
            </span>
          {/if}
        </span>
        <button
          class="rounded-lg bg-emerald-600 px-5 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
          onclick={handleClean}
        >
          Clean selected
        </button>
      </div>
    </div>
  {/if}

  <!-- Resource drill-down (get_resource_details, ids only) -->
  {#if scanStore.detail}
    <ResourceDetail
      resource={scanStore.detail}
      selected={scanStore.selected.includes(scanStore.detail.id)}
      onToggle={(id) => scanStore.toggle(id)}
      onClose={() => scanStore.closeDetail()}
    />
  {/if}

  <!-- Confirmation (G-UI-01) -->
  {#if cleanupStore.confirmOpen && cleanupStore.preview}
    <ConfirmDialog
      preview={cleanupStore.preview}
      measuredAt={cleanupStore.previewAt}
      showItems={settingsStore.settings?.show_preview ?? true}
      cleaning={cleanupStore.cleaning}
      cancelling={cleanupStore.cancelling}
      progress={cleanupStore.progress}
      onConfirm={() => cleanupStore.execute(scanStore.selected)}
      onCancel={() => cleanupStore.close()}
      onCancelCleanup={() => cleanupStore.cancel()}
    />
  {/if}

  <!-- Settings (scan roots + toggles, persisted to disk since M3) -->
  {#if settingsStore.open && settingsStore.settings}
    <SettingsDialog
      settings={settingsStore.settings}
      saving={settingsStore.saving}
      error={settingsStore.error}
      onSave={(next) => handleSaveSettings(next)}
      onClose={() => settingsStore.close()}
    />
  {/if}

  <!-- Cleanup history (backend JSONL log) -->
  {#if historyStore.open}
    <HistoryDialog
      entries={historyStore.entries}
      loading={historyStore.loading}
      error={historyStore.error}
      onRefresh={() => historyStore.refresh()}
      onClose={() => historyStore.close()}
    />
  {/if}
</div>
