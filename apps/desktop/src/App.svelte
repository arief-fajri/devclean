<script lang="ts">
  import { onMount } from "svelte";
  import {
    History,
    Leaf,
    Moon,
    RefreshCw,
    Settings,
    ShieldCheck,
    Sun,
    TriangleAlert,
  } from "lucide-svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import HistoryDialog from "$lib/components/HistoryDialog.svelte";
  import ListToolbar from "$lib/components/ListToolbar.svelte";
  import ProviderCard from "$lib/components/ProviderCard.svelte";
  import ResourceDetail from "$lib/components/ResourceDetail.svelte";
  import ResultPanel from "$lib/components/ResultPanel.svelte";
  import SelectionBar from "$lib/components/SelectionBar.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import StatTile from "$lib/components/StatTile.svelte";
  import SummaryPanel from "$lib/components/SummaryPanel.svelte";
  import { isStaleSignal } from "$lib/format";
  import { cleanupStore } from "$lib/stores/cleanup.svelte";
  import { historyStore } from "$lib/stores/history.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";
  import { settingsStore } from "$lib/stores/settings.svelte";
  import { viewStore } from "$lib/stores/view.svelte";
  import { applyView, countBySafety } from "$lib/view";

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

  const iconBtn =
    "flex h-9 w-9 items-center justify-center rounded-lg border border-zinc-200 text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800";

  /** Provider groups after the global view (search/filter/sort). */
  const groups = $derived(
    scanStore.summary
      ? applyView(scanStore.summary, scanStore.selected, viewStore.options)
      : [],
  );
  const allResources = $derived(scanStore.summary?.resources ?? []);
  const totalReclaimable = $derived(scanStore.summary?.reclaimable_bytes ?? 0);
  const safeBytes = $derived(
    allResources
      .filter((r) => r.safety === "safe")
      .reduce((acc, r) => acc + r.reclaimable_bytes, 0),
  );
  const categoryCounts = $derived.by(() => {
    let safeCats = 0;
    let reviewCats = 0;
    for (const p of scanStore.summary?.providers ?? []) {
      const rs = allResources.filter((r) => r.provider === p.provider);
      if (rs.length === 0) continue;
      if (rs.every((r) => r.safety === "safe")) safeCats += 1;
      else reviewCats += 1;
    }
    return { safeCats, reviewCats };
  });
  const statusLabel = $derived(
    scanStore.scanning ? "Scanning…" : scanStore.fromCache ? "Cached" : "Up to date",
  );
  const scanStatus = $derived(
    !scanStore.scanning
      ? null
      : scanStore.progress
        ? `Scanning ${scanStore.progress.provider}… (${scanStore.progress.done}/${scanStore.progress.total})`
        : scanStore.cancelling
          ? "Cancelling scan…"
          : "Starting scan…",
  );
  const scanPct = $derived(
    scanStore.progress && scanStore.progress.total > 0
      ? Math.round((scanStore.progress.done / scanStore.progress.total) * 100)
      : 0,
  );
  const impactById = $derived(
    new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r.impact])),
  );
  /** Safe/review split of the current selection (by id, filter-independent). */
  const selectionSafety = $derived.by(() => {
    const map = new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r]));
    let safe = 0;
    let review = 0;
    for (const id of scanStore.selected) {
      const r = map.get(id);
      if (!r) continue;
      if (r.safety === "safe") safe += 1;
      else review += 1;
    }
    return { safe, review };
  });
  const listCounts = $derived(countBySafety(allResources));
  const staleTotal = $derived(allResources.filter((r) => isStaleSignal(r.detail)).length);
  const resultCount = $derived(groups.reduce((acc, g) => acc + g.resources.length, 0));
  /** Visible, safe, not-yet-selected ids for "Select safe items". */
  const selectableSafeIds = $derived(
    groups
      .flatMap((g) => g.resources)
      .filter((r) => r.safety === "safe" && !scanStore.selected.includes(r.id))
      .map((r) => r.id),
  );
  const barBusy = $derived(scanStore.scanning || cleanupStore.cleaning);
</script>

<div class="min-h-screen bg-zinc-50 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
  <div class="mx-auto max-w-5xl px-4 pb-32 pt-6">
    <!-- Header -->
    <header class="flex items-center justify-between gap-3">
      <div class="flex items-center gap-2.5">
        <span
          aria-hidden="true"
          class="flex h-9 w-9 items-center justify-center rounded-lg bg-emerald-100 dark:bg-emerald-950"
        >
          <Leaf size={20} class="text-emerald-700 dark:text-emerald-300" />
        </span>
        <span>
          <span class="block text-xl leading-6 font-bold">DevClean</span>
          <span class="block text-xs text-zinc-500">Developer Storage Manager</span>
        </span>
      </div>
      <div class="flex items-center gap-2">
        <span
          class="mr-1 hidden items-center gap-1.5 rounded-full border border-zinc-200 px-2.5 py-1 text-xs text-zinc-500 sm:inline-flex dark:border-zinc-700"
          role="status"
        >
          <span
            aria-hidden="true"
            class="h-1.5 w-1.5 rounded-full {scanStore.scanning
              ? 'animate-pulse bg-amber-500'
              : 'bg-emerald-500'}"
          ></span>
          {statusLabel}
        </span>
        <button
          class={iconBtn}
          onclick={() => historyStore.openDialog()}
          aria-label="Open cleanup history"
          title="Cleanup history"
        >
          <span aria-hidden="true" class="flex"><History size={16} /></span>
        </button>
        <button
          class={iconBtn}
          onclick={() => settingsStore.openDialog()}
          aria-label="Open settings"
          title="Settings"
        >
          <span aria-hidden="true" class="flex"><Settings size={16} /></span>
        </button>
        <button
          class={iconBtn}
          onclick={toggleDark}
          aria-label="Toggle dark mode"
          title="Toggle dark mode"
        >
          <span aria-hidden="true" class="flex">
            {#if dark}<Sun size={16} />{:else}<Moon size={16} />{/if}
          </span>
        </button>
      </div>
    </header>

    {#if scanStore.error}
      <p
        class="mt-4 rounded-lg bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950 dark:text-red-300"
      >
        Scan error: {scanStore.error}
      </p>
    {/if}

    <!-- Summary dashboard -->
    {#if scanStore.summary}
      <div class="mt-6">
        <SummaryPanel
          reclaimable={scanStore.summary.reclaimable_bytes}
          resourceCount={scanStore.summary.resources.length}
          providerCount={scanStore.summary.providers.length}
          lastScannedAt={scanStore.lastScannedAt}
          fromCache={scanStore.fromCache}
          scanning={scanStore.scanning}
          cancelling={scanStore.cancelling}
          scanStatus={scanStatus}
          scanPct={scanPct}
          safeBytes={safeBytes}
          safeCategories={categoryCounts.safeCats}
          reviewCategories={categoryCounts.reviewCats}
          onScan={() => scanStore.start()}
          onCancelScan={() => scanStore.cancel()}
        />
      </div>

      <div class="mt-3 grid grid-cols-2 gap-3">
        <StatTile
          icon={ShieldCheck}
          value={String(categoryCounts.safeCats)}
          label="Safe categories"
          iconClass="text-emerald-600 dark:text-emerald-400"
          active={viewStore.safety === "safe"}
          onSelect={() => {
            viewStore.safety = viewStore.safety === "safe" ? "all" : "safe";
          }}
        />
        <StatTile
          icon={TriangleAlert}
          value={String(categoryCounts.reviewCats)}
          label="Need review"
          iconClass="text-amber-600 dark:text-amber-400"
          active={viewStore.safety === "review"}
          onSelect={() => {
            viewStore.safety = viewStore.safety === "review" ? "all" : "review";
          }}
        />
      </div>

      <div class="mt-4">
        <ListToolbar
          totalCount={allResources.length}
          safeCount={listCounts.safe}
          reviewCount={listCounts.review}
          staleCount={staleTotal}
          selectedCount={scanStore.selected.length}
          resultCount={resultCount}
          selectableSafe={selectableSafeIds.length}
          onSelectSafe={() => scanStore.selectMany(selectableSafeIds)}
        />
      </div>

      <!-- Provider breakdown (global view: search/filter/sort) -->
      <div class="mt-4 space-y-3">
        {#each groups as g (g.summary.provider)}
          <ProviderCard
            summary={g.summary}
            resources={g.resources}
            selected={scanStore.selected}
            totalBytes={totalReclaimable}
            expanded={scanStore.expanded.includes(g.summary.provider)}
            rescanning={scanStore.rescanning === g.summary.provider}
            onToggle={(id) => scanStore.toggle(id)}
            onToggleProvider={(prov) => scanStore.toggleProvider(prov)}
            onExpand={(prov) => scanStore.toggleExpand(prov)}
            onRescan={(prov) => scanStore.rescanProvider(prov)}
            onOpenDetail={(id) => scanStore.openDetail(id)}
          />
        {/each}
      </div>

      {#if groups.length === 0 && scanStore.summary.providers.length > 0}
        <div
          class="mt-6 rounded-xl border border-dashed border-zinc-300 p-6 text-center dark:border-zinc-700"
        >
          <p class="text-sm text-zinc-500">No resources match the current filters.</p>
          <button
            class="mt-3 rounded-lg border border-zinc-200 px-4 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
            onclick={() => viewStore.reset()}
          >
            Clear filters
          </button>
        </div>
      {/if}

      {#if scanStore.summary.providers.length === 0}
        <div
          class="mt-6 rounded-xl border border-dashed border-zinc-300 p-6 text-center dark:border-zinc-700"
        >
          <p class="text-sm font-medium text-zinc-700 dark:text-zinc-300">
            Nothing reclaimable found
          </p>
          <p class="mt-1 text-sm text-zinc-500">
            Add a scan root in Settings (defaults: ~/Projects, ~/Code, ~/Workspace) and scan
            again.
          </p>
          <button
            class="mt-3 inline-flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
            onclick={() => scanStore.start()}
            disabled={scanStore.scanning}
          >
            <span aria-hidden="true" class="flex"><RefreshCw size={15} /></span>
            Scan again
          </button>
        </div>
      {/if}
    {:else if !scanStore.scanning}
      <div class="mt-6 rounded-xl border border-dashed border-zinc-300 p-6 text-center dark:border-zinc-700">
        <p class="text-sm text-zinc-500">Press Scan to analyze your dev caches.</p>
        <button
          class="mt-3 inline-flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
          onclick={() => scanStore.start()}
        >
          <span aria-hidden="true" class="flex"><RefreshCw size={15} /></span>
          Scan now
        </button>
      </div>
    {:else}
      <p class="mt-6 text-center text-sm text-zinc-500" role="status">
        {scanStatus ?? "Starting scan…"}
      </p>
    {/if}

    <!-- Result: per-resource before/after (I8, G-REL-02) -->
    {#if cleanupStore.results}
      <ResultPanel
        results={cleanupStore.results}
        requestedBytes={cleanupStore.requestedBytes()}
        freedBytes={cleanupStore.freedBytes()}
        scanning={scanStore.scanning}
        nameOf={(id) => cleanupStore.nameOf(id)}
        onRescan={() => scanStore.start()}
        onDone={() => cleanupStore.dismissResults()}
      />
    {/if}
    {#if cleanupStore.error}
      <p class="mt-4 rounded-lg bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
        Cleanup error: {cleanupStore.error}
      </p>
    {/if}
  </div>

  <!-- Selection-aware cleanup bar -->
  {#if scanStore.selected.length > 0}
    <SelectionBar
      count={scanStore.selected.length}
      bytes={scanStore.selectedBytes()}
      safe={selectionSafety.safe}
      review={selectionSafety.review}
      busy={barBusy}
      onClear={() => scanStore.clearSelection()}
      onClean={handleClean}
    />
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
      lookupImpact={(id) => impactById.get(id) ?? undefined}
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
