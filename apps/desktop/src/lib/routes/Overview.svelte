<script lang="ts">
  /**
   * Overview page — the landing page. One screen that answers "how much can I
   * get back, what is holding the disk, and what happened last time", using
   * only data the app already has: the last scan summary, the free-space
   * probe loaded at boot, and the newest cleanup run.
   */
  import PageHeader from "$lib/components/PageHeader.svelte";
  import SummaryPanel from "$lib/components/SummaryPanel.svelte";
  import { formatBytes, formatPct, volumeLabel } from "$lib/format";
  import { runTotals, when } from "$lib/history";
  import { routeDef } from "$lib/nav";
  import { providerMeta } from "$lib/providerMeta";
  import { historyStore } from "$lib/stores/history.svelte";
  import { navStore } from "$lib/stores/nav.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";
  import { settingsStore } from "$lib/stores/settings.svelte";
  import { viewStore } from "$lib/stores/view.svelte";
  import { topProviders as topProvidersOf, isStaleActivity } from "$lib/view";
  import { Boxes, ChevronRight, Clock, HardDrive, RefreshCw, TriangleAlert } from "lucide-svelte";

  const route = routeDef("overview");

  // The newest run is the only history this page needs; `ensureLoaded` keeps
  // it to one call per visit instead of a poll.
  $effect(() => {
    void historyStore.ensureLoaded();
  });

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
    return { safe: safeCats, review: reviewCats };
  });

  const totalProviders = $derived(
    settingsStore.providers.filter((p) => p.enabled).length,
  );

  const scanStatus = $derived.by(() => {
    if (!scanStore.scanning) return null;
    if (scanStore.cancelling) return "Cancelling scan…";
    const p = scanStore.progress;
    if (!p) return "Starting scan…";
    return `Scanning ${providerMeta(p.provider).label}… (${p.done}/${p.total})`;
  });

  const scanPct = $derived(
    scanStore.progress && scanStore.progress.total > 0
      ? Math.round((scanStore.progress.done / scanStore.progress.total) * 100)
      : 0,
  );

  /** Biggest reclaimers first — the answer to "where is my disk going?".
   *  Capped, with the tail aggregated (never silently dropped). */
  const topProviders = $derived(
    scanStore.summary ? topProvidersOf(scanStore.summary) : [],
  );
  const topMax = $derived(topProviders[0]?.bytes ?? 0);

  const stale = $derived(allResources.filter((r) => isStaleActivity(r.activity)));
  const oldestStale = $derived(
    stale.reduce((acc, r) => Math.max(acc, r.age_days ?? 0), 0),
  );

  const diskFree = $derived(settingsStore.diskFree);
  const freePct = $derived(
    diskFree?.known && diskFree.total_bytes > 0
      ? (diskFree.available_bytes / diskFree.total_bytes) * 100
      : 0,
  );

  const lastRun = $derived(historyStore.runs[0] ?? null);
  const lastRunFreed = $derived(lastRun ? runTotals([lastRun]).freed : 0);

  /** Jump to the list with the matching filter already applied. */
  function showResources(filter?: "safe" | "stale") {
    if (filter === "safe") {
      viewStore.safety = "safe";
      viewStore.staleOnly = false;
    } else if (filter === "stale") {
      viewStore.safety = "all";
      viewStore.staleOnly = true;
    }
    navStore.navigate("resources");
  }
</script>

<PageHeader title={route.label} hint={route.hint} />

{#if scanStore.summary}
  <SummaryPanel
    reclaimable={scanStore.summary.reclaimable_bytes}
    resourceCount={scanStore.summary.resources.length}
    providerCount={scanStore.summary.providers.length}
    lastScannedAt={scanStore.lastScannedAt}
    fromCache={scanStore.fromCache}
    partialScan={scanStore.partialScan}
    scanning={scanStore.scanning}
    cancelling={scanStore.cancelling}
    {scanStatus}
    {scanPct}
    activeProvider={scanStore.activeProvider}
    doneProviders={scanStore.doneProviders}
    {totalProviders}
    {safeBytes}
    safeCategories={categoryCounts.safe}
    reviewCategories={categoryCounts.review}
    blockedBytes={scanStore.summary.blocked_bytes}
    uniqueBytes={scanStore.summary.unique_bytes ?? null}
    onScan={() => scanStore.start()}
    onCancelScan={() => scanStore.cancel()}
  />

  <div class="mt-4 grid gap-4 lg:grid-cols-2">
    <!-- Biggest reclaimers -->
    <section
      class="flex flex-col rounded-2xl border border-zinc-200 bg-white p-5 dark:border-zinc-800 dark:bg-zinc-900"
      aria-label="Top providers by reclaimable size"
    >
      <h2 class="flex items-center gap-2 text-sm font-semibold text-zinc-700 dark:text-zinc-200">
        <span aria-hidden="true" class="flex text-zinc-400"><Boxes size={15} /></span>
        Where the space is
      </h2>
      {#if topProviders.length === 0}
        <p class="mt-3 text-sm text-zinc-500">Nothing reclaimable in the last scan.</p>
      {:else}
        <ul class="mt-3 flex-1 space-y-2.5">
          {#each topProviders as row (row.provider ?? row.label)}
            {@const meta = row.isOther ? null : providerMeta(row.provider ?? "")}
            {@const Icon = meta?.icon}
            <li>
              <div class="flex items-baseline justify-between gap-2 text-xs">
                <span
                  class="inline-flex min-w-0 items-center gap-1.5 {row.isOther
                    ? 'text-zinc-500'
                    : 'text-zinc-700 dark:text-zinc-300'}"
                >
                  {#if meta && Icon}
                    <span aria-hidden="true" class="flex shrink-0 {meta.accent}"
                      ><Icon size={13} /></span
                    >
                  {:else}
                    <span aria-hidden="true" class="flex shrink-0 text-zinc-400"
                      ><Boxes size={13} /></span
                    >
                  {/if}
                  <span class="truncate">{row.label}</span>
                </span>
                <span class="tnum shrink-0 font-mono text-zinc-500">
                  {formatBytes(row.bytes)}
                </span>
              </div>
              <div class="mt-1 h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
                <div
                  class="h-full rounded-full {row.isOther ? 'bg-zinc-400' : 'bg-emerald-500'}"
                  style="width: {topMax > 0 ? (row.bytes / topMax) * 100 : 0}%"
                ></div>
              </div>
            </li>
          {/each}
        </ul>
        <button
          class="mt-4 inline-flex items-center gap-1 self-start text-xs font-medium text-emerald-700 hover:underline dark:text-emerald-300"
          onclick={() => navStore.navigate("resources")}
        >
          Review resources
          <span aria-hidden="true" class="flex"><ChevronRight size={13} /></span>
        </button>
      {/if}
    </section>

    <div class="space-y-4">
      <!-- Free space: a full disk is the failure this app exists to prevent -->
      {#if diskFree?.known}
        {@const low = freePct < 10}
        <section
          class="rounded-2xl border p-5 {low
            ? 'border-amber-300 bg-amber-50 dark:border-amber-800 dark:bg-amber-950/30'
            : 'border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900'}"
          aria-label="Free disk space"
        >
          <h2 class="flex items-center gap-2 text-sm font-semibold text-zinc-700 dark:text-zinc-200">
            <span aria-hidden="true" class="flex text-zinc-400"><HardDrive size={15} /></span>
            {volumeLabel(diskFree)}
          </h2>
          <p class="tnum mt-2 font-mono text-2xl font-semibold text-zinc-900 dark:text-zinc-50">
            {formatBytes(diskFree.available_bytes)}
          </p>
          <div class="mt-2 h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
            <div
              class="h-full rounded-full {low ? 'bg-amber-500' : 'bg-zinc-400'}"
              style="width: {Math.max(1, Math.min(100, freePct))}%"
            ></div>
          </div>
          <p class="mt-1.5 text-xs {low ? 'text-amber-800 dark:text-amber-200' : 'text-zinc-500'}">
            {formatPct(diskFree.available_bytes, diskFree.total_bytes)} of
            {formatBytes(diskFree.total_bytes)} · {low ? "running low" : "no immediate pressure"}
          </p>
        </section>
      {/if}

      <!-- Stale is a signal, never a verdict: it routes to the list, it does not clean -->
      <section
        class="rounded-2xl border border-zinc-200 bg-white p-5 dark:border-zinc-800 dark:bg-zinc-900"
        aria-label="Stale items"
      >
        <h2 class="flex items-center gap-2 text-sm font-semibold text-zinc-700 dark:text-zinc-200">
          <span aria-hidden="true" class="flex text-zinc-400"><Clock size={15} /></span>
          Stale for over 90 days
        </h2>
        {#if stale.length === 0}
          <p class="mt-2 text-sm text-zinc-500">Nothing old enough to flag.</p>
        {:else}
          <p class="tnum mt-2 font-mono text-2xl font-semibold text-zinc-900 dark:text-zinc-50">
            {stale.length}
            <span class="text-sm font-normal text-zinc-500">items</span>
          </p>
          <p class="mt-1 flex items-center gap-1.5 text-xs text-zinc-500">
            <span aria-hidden="true" class="flex text-amber-600"><TriangleAlert size={12} /></span>
            Oldest {oldestStale}d — a signal to review, not a licence to delete.
          </p>
          <button
            class="mt-3 inline-flex items-center gap-1 text-xs font-medium text-emerald-700 hover:underline dark:text-emerald-300"
            onclick={() => showResources("stale")}
          >
            Show only stale
            <span aria-hidden="true" class="flex"><ChevronRight size={13} /></span>
          </button>
        {/if}
      </section>

      <!-- Last cleanup: what was actually freed, per I12 (measured) -->
      <section
        class="rounded-2xl border border-zinc-200 bg-white p-5 dark:border-zinc-800 dark:bg-zinc-900"
        aria-label="Last cleanup"
      >
        <h2 class="text-sm font-semibold text-zinc-700 dark:text-zinc-200">Last cleanup</h2>
        {#if lastRun}
          <p class="tnum mt-2 font-mono text-2xl font-semibold text-zinc-900 dark:text-zinc-50">
            {formatBytes(lastRunFreed)}
          </p>
          <p class="mt-1 text-xs text-zinc-500">
            freed {when(lastRun.finished_at_unix)} · {lastRun.entry_count}
            {lastRun.entry_count === 1 ? "item" : "items"}
          </p>
        {:else}
          <p class="mt-2 text-sm text-zinc-500">No cleanup has run on this machine yet.</p>
        {/if}
        <button
          class="mt-3 inline-flex items-center gap-1 text-xs font-medium text-emerald-700 hover:underline dark:text-emerald-300"
          onclick={() => navStore.navigate("history")}
        >
          Full history
          <span aria-hidden="true" class="flex"><ChevronRight size={13} /></span>
        </button>
      </section>
    </div>
  </div>
{:else if !scanStore.scanning}
  <div
    class="mt-2 rounded-2xl border border-amber-300 bg-amber-50 p-8 text-center dark:border-amber-800 dark:bg-amber-950/30"
    role="status"
  >
    <p
      class="inline-flex items-center gap-2 text-sm font-medium text-amber-800 dark:text-amber-200"
    >
      <span aria-hidden="true" class="flex"><TriangleAlert size={15} /></span>
      You haven't scanned yet
    </p>
    <p class="mx-auto mt-2 max-w-md text-sm text-amber-900/80 dark:text-amber-100/80">
      DevClean measures what each provider holds before it can tell you what is safe to remove.
      Run a scan to see your reclaimable space.
    </p>
    <button
      class="mt-4 inline-flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
      onclick={() => scanStore.start()}
    >
      <span aria-hidden="true" class="flex"><RefreshCw size={15} /></span>
      Scan Now
    </button>
    <p class="mt-3 text-xs text-zinc-500">
      Nothing is deleted without your confirmation. Scan roots live in Settings.
    </p>
  </div>
{:else}
  <p class="mt-2 text-center text-sm text-zinc-500 flex items-center justify-center" style="height: calc(100% - 200px);" role="status">
    {scanStatus ?? "Starting scan…"}
  </p>
{/if}
