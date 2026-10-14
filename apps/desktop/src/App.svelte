<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { CircleCheck, History, Moon, RefreshCw, Settings, Sun } from "lucide-svelte";
  import AppLogo from "$lib/components/AppLogo.svelte";
  import CleanupFlowDialog from "$lib/components/CleanupFlowDialog.svelte";
  import HistoryDialog from "$lib/components/HistoryDialog.svelte";
  import ListToolbar from "$lib/components/ListToolbar.svelte";
  import ProviderCard from "$lib/components/ProviderCard.svelte";
  import ResourceDetail from "$lib/components/ResourceDetail.svelte";
  import SelectionBar from "$lib/components/SelectionBar.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import SummaryPanel from "$lib/components/SummaryPanel.svelte";
  import { isStaleSignal } from "$lib/format";
  import { cleanupStore } from "$lib/stores/cleanup.svelte";
  import { historyStore } from "$lib/stores/history.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";
  import { settingsStore } from "$lib/stores/settings.svelte";
  import { viewStore } from "$lib/stores/view.svelte";
  import { providerMeta } from "$lib/providerMeta";
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
    const providers = providersOf(scanStore.selected);
    if (s && !s.require_confirmation) {
      cleanupStore.touchedProviders = providers;
      void cleanupStore.execute(scanStore.selected);
    } else {
      void cleanupStore.openConfirm(scanStore.selected);
    }
  }

  /** Re-scan only what was cleaned. Falls back to a full scan when we
   * cannot tell which providers were involved. */
  function handleRescanAfterCleanup() {
    const providers = cleanupStore.touchedProviders;
    cleanupStore.closeFlow();
    if (providers.length > 0) void scanStore.rescanProviders(providers);
    else void scanStore.start();
  }

  function handleConfirmClean() {
    cleanupStore.touchedProviders = providersOf(scanStore.selected);
    void cleanupStore.execute(scanStore.selected);
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
  // Forget expanded ids whose provider no longer has any item, so a card
  // can never appear to open itself after a cleanup. Reads only `groups`
  // and writes the view store via untrack → no self-triggering loop.
  $effect(() => {
    const present = groups.map((g) => g.summary.provider);
    untrack(() => viewStore.pruneExpanded(present));
  });

  /** Providers just cleaned that have nothing left. Rendered as a compact
   * note so the list does not silently jump when their card disappears. */
  const cleanedGone = $derived(
    cleanupStore.touchedProviders.filter(
      (p) => !groups.some((g) => g.summary.provider === p),
    ),
  );
  function providerName(id: string): string {
    return settingsStore.providers.find((p) => p.id === id)?.name ?? id;
  }

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
  /** Every provider this run covers — disabled ones never take a slot. */
  const totalProviders = $derived(
    settingsStore.providers.filter((p) => p.enabled).length,
  );
  const scanStatus = $derived.by(() => {
    if (!scanStore.scanning) return null;
    if (scanStore.cancelling) return "Cancelling scan…";
    const p = scanStore.progress;
    if (!p) return "Starting scan…";
    const name = providerMeta(p.provider).label;
    return `Scanning ${name}… (${p.done}/${p.total})`;
  });
  const scanPct = $derived(
    scanStore.progress && scanStore.progress.total > 0
      ? Math.round((scanStore.progress.done / scanStore.progress.total) * 100)
      : 0,
  );
  const impactById = $derived(
    new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r.impact])),
  );
  const providerById = $derived(
    new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r.provider])),
  );
  const pathById = $derived(
    new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r.path])),
  );
  /** Provider ids behind a set of resource ids (ids only — never paths). */
  function providersOf(ids: string[]): string[] {
    const out = new Set<string>();
    for (const id of ids) {
      const p = providerById.get(id);
      if (p) out.add(p);
    }
    return [...out];
  }
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
  /** Per-provider item count + bytes from the last scan (Settings hint). */
  const providerStats = $derived.by(() => {
    const out: Record<string, { count: number; bytes: number }> = {};
    for (const p of scanStore.summary?.providers ?? []) {
      out[p.provider] = { count: p.resource_count, bytes: p.reclaimable_bytes };
    }
    return out;
  });
</script>

<div class="min-h-screen bg-zinc-50 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
  <div class="mx-auto max-w-5xl px-4 pb-32 xl:max-w-7xl">
    <!-- Header: sticky so Scan/Settings stay reachable on long lists -->
    <header
      class="sticky top-0 z-30 -mx-4 border-b border-transparent bg-zinc-50/90 px-4 pt-6 pb-3 backdrop-blur dark:bg-zinc-950/90"
    >
      <div class="flex items-center justify-between gap-3">
      <div class="flex items-center gap-2.5">
        <AppLogo size={36} />
        <span>
          <span class="block text-xl leading-6 font-bold">DevClean</span>
          <span class="block text-xs text-zinc-500">Developer Storage Manager</span>
        </span>
      </div>
      <div class="flex items-center gap-2">
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
          partialScan={scanStore.partialScan}
          scanning={scanStore.scanning}
          cancelling={scanStore.cancelling}
          scanStatus={scanStatus}
          scanPct={scanPct}
          activeProvider={scanStore.activeProvider}
          doneProviders={scanStore.doneProviders}
          totalProviders={totalProviders}
          safeBytes={safeBytes}
          safeCategories={categoryCounts.safeCats}
          reviewCategories={categoryCounts.reviewCats}
          retentionBlocked={scanStore.summary.retention_blocked_bytes}
          retentionDays={settingsStore.settings?.retention_days ?? null}
          onScan={() => scanStore.start()}
          onCancelScan={() => scanStore.cancel()}
        />
      </div>

      <div class="mt-3">
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
            showPath={viewStore.showPaths}
            expanded={viewStore.expanded.includes(g.summary.provider)}
            rescanning={scanStore.rescanning === g.summary.provider}
            scanning={scanStore.scanning}
            active={scanStore.activeProvider === g.summary.provider}
            done={scanStore.doneProviders.includes(g.summary.provider)}
            onToggle={(id) => scanStore.toggle(id)}
            onToggleProvider={(prov) => scanStore.toggleProvider(prov)}
            onExpand={(prov) => viewStore.toggleExpand(prov)}
            onRescan={(prov) => scanStore.rescanProvider(prov)}
            onOpenDetail={(id) => scanStore.openDetail(id)}
          />
        {/each}

        <!-- Providers emptied by the last cleanup: say so instead of
             letting the list silently jump. -->
        {#each cleanedGone as p (p)}
          <div
            class="flex items-center gap-2 rounded-xl border border-dashed border-zinc-200 px-4 py-3 text-sm text-zinc-500 dark:border-zinc-800"
            data-testid="cleaned-empty-{p}"
          >
            <span aria-hidden="true" class="flex shrink-0 text-emerald-600 dark:text-emerald-400">
              <CircleCheck size={16} />
            </span>
            <span class="min-w-0 flex-1 truncate">{providerName(p)}</span>
            <span class="shrink-0 text-xs text-zinc-400">Cleaned · nothing left</span>
          </div>
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
      disabled={scanStore.scanning}
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

  <!-- Cleanup flow: review → running → done (G-UI-01, I8, G-REL-02).
       Stays mounted through running into done; closes via Done/Rescan. -->
  {#if cleanupStore.confirmOpen || cleanupStore.cleaning || cleanupStore.results}
    <CleanupFlowDialog
      preview={cleanupStore.preview}
      measuredAt={cleanupStore.previewAt}
      showItems={settingsStore.settings?.show_preview ?? true}
      cleaning={cleanupStore.cleaning}
      cancelling={cleanupStore.cancelling}
      progress={cleanupStore.progress}
      results={cleanupStore.results}
      lookupImpact={(id) => impactById.get(id) ?? undefined}
      lookupPath={(id) => pathById.get(id)}
      nameOf={(id) => cleanupStore.nameOf(id)}
      onConfirm={handleConfirmClean}
      onCancel={() => cleanupStore.close()}
      onCancelCleanup={() => cleanupStore.cancel()}
      onRescan={handleRescanAfterCleanup}
      onDone={() => cleanupStore.closeFlow()}
    />
  {/if}

  <!-- Settings (scan roots + toggles, persisted to disk since M3) -->
  {#if settingsStore.open && settingsStore.settings}
    <SettingsDialog
      settings={settingsStore.settings}
      providers={settingsStore.providers}
      {providerStats}
      saving={settingsStore.saving}
      error={settingsStore.error}
      onSave={(next) => handleSaveSettings(next)}
      onClose={() => settingsStore.close()}
    />
  {/if}

  <!-- Cleanup history (backend JSONL log) -->
  {#if historyStore.open}
    <HistoryDialog
      runs={historyStore.runs}
      truncated={historyStore.truncated}
      loading={historyStore.loading}
      error={historyStore.error}
      onRefresh={() => historyStore.refresh()}
      onClose={() => historyStore.close()}
    />
  {/if}
</div>
