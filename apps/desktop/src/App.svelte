<script lang="ts">
  /**
   * App shell. Owns the sidebar, the page switch, the global selection bar,
   * the resource drawer and the cleanup flow.
   *
   * Layer order, deliberately: the cleanup flow is the only modal (G-UI-03 —
   * a destructive confirmation is an interruption, never a page), the drawer
   * is non-modal, and everything else is plain page content.
   */
  import { onMount, untrack } from "svelte";
  import { viewStore } from "$lib/stores/view.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import CleanupFlowDialog from "$lib/components/CleanupFlowDialog.svelte";
  import HistoryPage from "$lib/routes/HistoryPage.svelte";
  import Overview from "$lib/routes/Overview.svelte";
  import Resources from "$lib/routes/Resources.svelte";
  import ResourceDrawer from "$lib/components/ResourceDrawer.svelte";
  import SelectionBar from "$lib/components/SelectionBar.svelte";
  import SettingsPage from "$lib/routes/SettingsPage.svelte";
  import { isRouteId, resolveHotkey } from "$lib/nav";
  import { cleanupStore } from "$lib/stores/cleanup.svelte";
  import { navStore } from "$lib/stores/nav.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";
  import { settingsStore } from "$lib/stores/settings.svelte";
  import { onTrayAction } from "$lib/tauri";
  import type { RouteId } from "$lib/nav";

  let dark = $state(false);

  function applyTheme(theme: string | undefined) {
    const sysDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    dark = theme === "dark" || (theme !== "light" && sysDark);
    document.documentElement.classList.toggle("dark", dark);
  }

  onMount(() => {
    scanStore.listen();
    cleanupStore.listen();
    void onTrayAction((action) => {
      // The menu bar may only ask for a page; it can never clean. An id we do
      // not know is ignored rather than guessed at.
      if (!isRouteId(action.route)) return;
      if (action.filter === "safe") viewStore.safety = "safe";
      navStore.open(action.route);
    });
    // Settings first (persisted theme), then instant cache paint,
    // then an honest background rescan.
    void (async () => {
      await settingsStore.load();
      applyTheme(settingsStore.settings?.theme);
      await scanStore.loadCached();
      await scanStore.start();
    })();
  });

  function toggleDark() {
    const next: "dark" | "light" = dark ? "light" : "dark";
    applyTheme(next);
    void settingsStore.setTheme(next);
  }

  /** Clean entry point: confirmation dialog by default (G-UI-01).
   * Only skipped when the user explicitly disabled require_confirmation. */
  function handleClean() {
    const s = settingsStore.settings;
    const providers = providersOf(scanStore.selected);
    if (s && !s.require_confirmation) {
      cleanupStore.touchedProviders = providers;
      void cleanupStore.execute(scanStore.selected);
    } else {
      // opens the dialog immediately; the re-measure runs behind it
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

  const impactById = $derived(
    new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r.impact])),
  );
  const pathById = $derived(
    new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r.path])),
  );
  const providerById = $derived(
    new Map((scanStore.summary?.resources ?? []).map((r) => [r.id, r.provider])),
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

  const barBusy = $derived(scanStore.scanning || cleanupStore.cleaning);
  // The flow dialog owns the whole path: it opens when the user asks for a
  // review, stays open through the re-measure and any failure, and closes only
  // on Done/Rescan.
  const flowOpen = $derived(cleanupStore.confirmOpen);

  function onKeydown(e: KeyboardEvent) {
    const action = resolveHotkey(e);
    if (!action) return;
    // G-UI-03: while the confirmation is open the shell keeps its hands off
    // the keyboard — Escape belongs to the dialog, and a shortcut must never
    // navigate away from an unreviewed destruction.
    if (flowOpen) return;
    e.preventDefault();
    if (action.kind === "route") navStore.navigate(action.route);
    else if (action.kind === "scan") void scanStore.start();
    else if (action.kind === "settings") navStore.navigate("settings");
    else navStore.focusSearch();
  }

  /* Each page keeps its own scroll offset: coming back to a long list should
     land where the user left it, not at the top. */
  let scroller = $state<HTMLDivElement | null>(null);
  let lastRoute: RouteId = navStore.route;
  $effect(() => {
    const route = navStore.route;
    const el = scroller;
    if (!el) return;
    untrack(() => {
      if (route !== lastRoute) {
        navStore.saveScroll(lastRoute, el.scrollTop);
        lastRoute = route;
        el.scrollTop = navStore.scroll[route] ?? 0;
      }
    });
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-screen overflow-hidden bg-zinc-50 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
  <Sidebar {dark} onToggleDark={toggleDark} />

  <main class="flex min-w-0 flex-1 flex-col">
    <div
      bind:this={scroller}
      class="min-h-0 flex-1 overflow-y-auto px-6 pb-8 xl:px-10"
      class:pb-32={scanStore.selected.length > 0 && navStore.route !== "settings"}
    >
      {#if scanStore.error}
        <p
          class="mt-4 rounded-lg bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950 dark:text-red-300"
          role="alert"
        >
          Scan error: {scanStore.error}
        </p>
      {/if}

      {#if navStore.route === "overview"}
        <Overview />
      {:else if navStore.route === "resources"}
        <Resources />
      {:else if navStore.route === "history"}
        <HistoryPage />
      {:else}
        <SettingsPage onSave={handleSaveSettings} />
      {/if}
    </div>
  </main>
</div>

<!-- Selection-aware cleanup bar. It follows the user across pages so a staged
     selection is never out of sight — except on Settings, where the fixed save
     bar already occupies that edge (two stacked bars read as noise). The
     selection itself is untouched and the bar returns with it. -->
{#if scanStore.selected.length > 0 && navStore.route !== "settings"}
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
  <ResourceDrawer
    resource={scanStore.detail}
    selected={scanStore.selected.includes(scanStore.detail.id)}
    onToggle={(id) => scanStore.toggle(id)}
    onClose={() => scanStore.closeDetail()}
  />
{/if}

<!-- Cleanup flow: review → running → done (G-UI-01, I8, G-REL-02).
     Stays mounted through running into done; closes via Done/Rescan. -->
{#if flowOpen}
  <CleanupFlowDialog
    preview={cleanupStore.preview}
    previewing={cleanupStore.previewing}
    previewError={cleanupStore.previewError}
    selectedCount={scanStore.selected.length}
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
    onRetry={() => cleanupStore.retryPreview(scanStore.selected)}
    onRescanFromPreview={() => {
      cleanupStore.close();
      void scanStore.start();
    }}
    onCancelCleanup={() => cleanupStore.cancel()}
    onRescan={handleRescanAfterCleanup}
    onDone={() => cleanupStore.closeFlow()}
  />
{/if}