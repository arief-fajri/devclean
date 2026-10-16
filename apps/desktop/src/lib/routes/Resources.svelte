<script lang="ts">
  /**
   * Resources page: the working list. Everything the old single-page app did
   * below its summary panel — search/filter/sort, the provider cards and
   * their item rows. Selection lives in `scanStore` and survives navigation,
   * so the selection bar stays valid on every page.
   */
  import { untrack } from "svelte";
  import ListToolbar from "$lib/components/ListToolbar.svelte";
  import PageHeader from "$lib/components/PageHeader.svelte";
  import ProviderCard from "$lib/components/ProviderCard.svelte";
  import { applyView, countBySafety, isStaleActivity } from "$lib/view";
  import { routeDef } from "$lib/nav";
  import { providerMeta } from "$lib/providerMeta";
  import { cleanupStore } from "$lib/stores/cleanup.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";
  import { settingsStore } from "$lib/stores/settings.svelte";
  import { viewStore } from "$lib/stores/view.svelte";
  import { CircleCheck, RefreshCw, TriangleAlert } from "lucide-svelte";

  const route = routeDef("resources");

  const allResources = $derived(scanStore.summary?.resources ?? []);
  const totalReclaimable = $derived(scanStore.summary?.reclaimable_bytes ?? 0);
  const groups = $derived(
    scanStore.summary ? applyView(scanStore.summary, scanStore.selected, viewStore.options) : [],
  );
  const listCounts = $derived(countBySafety(allResources));
  const staleTotal = $derived(allResources.filter((r) => isStaleActivity(r.activity)).length);
  const resultCount = $derived(groups.reduce((acc, g) => acc + g.resources.length, 0));

  /** Safe, not-yet-selected ids for "Select safe items". */
  const selectableSafeIds = $derived(
    groups
      .flatMap((g) => g.resources)
      .filter((r) => r.safety === "safe" && !scanStore.selected.includes(r.id))
      .map((r) => r.id),
  );

  /** Providers emptied by the last cleanup, so the list cannot silently jump. */
  const cleanedGone = $derived(
    cleanupStore.touchedProviders.filter(
      (p) => !groups.some((g) => g.summary.provider === p),
    ),
  );

  // Forget expanded ids whose provider no longer has any item. Reads `groups`
  // and writes the view store via untrack → no self-triggering loop.
  $effect(() => {
    const present = groups.map((g) => g.summary.provider);
    untrack(() => viewStore.pruneExpanded(present));
  });

  function providerName(id: string): string {
    return settingsStore.providers.find((p) => p.id === id)?.name ?? providerMeta(id).label;
  }
</script>

<PageHeader title={route.label} hint={route.hint}>
  {#snippet actions()}
    <button
      class="inline-flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
      onclick={() => scanStore.start()}
      disabled={scanStore.scanning}
    >
      <span aria-hidden="true" class="flex"><RefreshCw size={15} /></span>
      {scanStore.scanning ? "Scanning…" : "Scan again"}
    </button>
  {/snippet}
</PageHeader>

{#if scanStore.summary}
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
{:else if !scanStore.scanning}
  <div
    class="mt-2 rounded-xl border border-amber-300 bg-amber-50 p-6 text-center dark:border-amber-800 dark:bg-amber-950/30"
    role="status"
  >
    <p
      class="inline-flex items-center gap-2 text-sm font-medium text-amber-800 dark:text-amber-200"
    >
      <span aria-hidden="true" class="flex"><TriangleAlert size={15} /></span>
      You haven't scanned yet
    </p>
    <p class="mt-2 text-sm text-amber-900/80 dark:text-amber-100/80">
      Run a scan to list what is reclaimable.
    </p>
    <button
      class="mt-3 inline-flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
      onclick={() => scanStore.start()}
    >
      <span aria-hidden="true" class="flex"><RefreshCw size={15} /></span>
      Scan Now
    </button>
  </div>
{:else}
  <p class="mt-2 text-center text-sm text-zinc-500 flex items-center justify-center" style="height: calc(100% - 200px);" role="status">Scanning…</p>
{/if}
