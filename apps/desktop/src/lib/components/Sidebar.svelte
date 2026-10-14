<script lang="ts">
  /**
   * App shell sidebar: brand, the four pages, and a footer that always shows
   * what the backend is doing (scan progress) plus the theme switch.
   */
  import AppLogo from "$lib/components/AppLogo.svelte";
  import { ROUTES } from "$lib/nav";
  import { providerMeta } from "$lib/providerMeta";
  import { cleanupStore } from "$lib/stores/cleanup.svelte";
  import { navStore } from "$lib/stores/nav.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";
  import { LoaderCircle, Moon, Sun } from "lucide-svelte";

  interface Props {
    dark: boolean;
    onToggleDark: () => void;
  }

  let { dark, onToggleDark }: Props = $props();

  const progress = $derived(scanStore.progress);
  const scanLine = $derived(
    scanStore.scanning
      ? `${scanStore.cancelling ? "Cancelling" : "Scanning"}${
          progress ? ` ${progress.done}/${progress.total}` : ""
        }`
      : null,
  );
  const idleLine = $derived(
    scanStore.lastScannedAt
      ? `Last scan ${scanStore.lastScannedAt.toLocaleTimeString()}`
      : "Never scanned",
  );
</script>

<aside
  class="flex w-sidebar shrink-0 flex-col border-r border-zinc-200 bg-zinc-100/60 dark:border-zinc-800 dark:bg-zinc-900/40"
>
  <div class="flex items-center gap-2.5 px-4 pt-5 pb-4">
    <AppLogo size={32} />
    <div class="min-w-0">
      <p class="truncate text-base leading-5 font-bold text-zinc-900 dark:text-zinc-50">DevClean</p>
      <p class="truncate text-[11px] text-zinc-500">Developer Storage Manager</p>
    </div>
  </div>

  <nav class="flex-1 overflow-y-auto px-3" aria-label="Pages">
    <ul class="space-y-0.5">
      {#each ROUTES as route (route.id)}
        {@const active = navStore.route === route.id}
        {@const Icon = route.icon}
        <li>
          <button
            class="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-sm font-medium transition-colors {active
              ? 'bg-zinc-900 text-white dark:bg-zinc-100 dark:text-zinc-900'
              : 'text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800'}"
            aria-current={active ? "page" : undefined}
            onclick={() => navStore.navigate(route.id)}
          >
            <span aria-hidden="true" class="flex shrink-0"><Icon size={16} /></span>
            <span class="min-w-0 flex-1 truncate text-left">{route.label}</span>
            {#if route.id === "resources" && scanStore.selected.length > 0}
              <span
                class="tnum rounded-full bg-emerald-600 px-1.5 py-0.5 text-[10px] font-semibold text-white"
                >{scanStore.selected.length}</span
              >
            {:else if active}
              <kbd class="text-[10px] text-zinc-400 dark:text-zinc-500">⌘{route.digit}</kbd>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="space-y-2 px-3 py-4">
    <!-- Scan state is never a surprise: the footer says what is running even
         when the user is on another page. -->
    <p
      class="flex items-center gap-1.5 px-1 text-[11px] text-zinc-500"
      role="status"
      aria-live="polite"
      data-testid="sidebar-scan-state"
    >
      {#if scanLine}
        <span aria-hidden="true" class="flex animate-spin text-emerald-600"
          ><LoaderCircle size={12} /></span
        >
        <span class="truncate">{scanLine}</span>
        {#if scanStore.activeProvider}
          <span class="truncate">· {providerMeta(scanStore.activeProvider).label}</span>
        {/if}
      {:else if cleanupStore.cleaning}
        <span class="truncate">Cleaning…</span>
      {:else}
        <span class="truncate">{idleLine}</span>
      {/if}
    </p>

    <button
      class="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
      onclick={onToggleDark}
      aria-label="Toggle dark mode"
      title="Toggle dark mode"
    >
      <span aria-hidden="true" class="flex shrink-0">
        {#if dark}<Sun size={16} />{:else}<Moon size={16} />{/if}
      </span>
      <span>{dark ? "Light theme" : "Dark theme"}</span>
    </button>
  </div>
</aside>
