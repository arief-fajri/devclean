<script lang="ts">
  import ResourceRow from "$lib/components/ResourceRow.svelte";
  import { formatBytes, formatPct, isStaleSignal } from "$lib/format";
  import { providerMeta } from "$lib/providerMeta";
  import type { ProviderSummary, Resource } from "$lib/tauri";
  import { ChevronDown, RotateCw } from "lucide-svelte";

  interface Props {
    summary: ProviderSummary;
    /** Visible rows only (global filter already applied by the caller). */
    resources: Resource[];
    selected: string[];
    /** Total reclaimable across ALL providers — the bar scale. */
    totalBytes: number;
    showPath: boolean;
    expanded: boolean;
    rescanning: boolean;
    onToggle: (id: string) => void;
    onToggleProvider: (provider: string) => void;
    onExpand: (provider: string) => void;
    onRescan: (provider: string) => void;
    onOpenDetail: (id: string) => void;
  }

  let {
    summary,
    resources,
    selected,
    totalBytes,
    showPath,
    expanded,
    rescanning,
    onToggle,
    onToggleProvider,
    onExpand,
    onRescan,
    onOpenDetail,
  }: Props = $props();

  const meta = $derived(providerMeta(summary.provider));
  const TileIcon = $derived(meta.icon);
  const pct = $derived(formatPct(summary.reclaimable_bytes, totalBytes));
  const barPct = $derived(
    totalBytes > 0 ? Math.max(2, Math.min(100, (summary.reclaimable_bytes / totalBytes) * 100)) : 0,
  );
  const staleCount = $derived(resources.filter((r) => isStaleSignal(r.detail)).length);
  const reviewCount = $derived(resources.filter((r) => r.safety !== "safe").length);
  const allChecked = $derived(
    resources.length > 0 && resources.every((r) => selected.includes(r.id)),
  );
  const barColor = $derived(reviewCount > 0 ? "bg-amber-500" : "bg-emerald-500");
</script>

<section
  class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
>
  <div class="flex items-start gap-3">
    <input
      type="checkbox"
      class="mt-2.5 h-4 w-4 shrink-0 accent-emerald-600"
      checked={allChecked}
      onchange={() => onToggleProvider(summary.provider)}
      aria-label="Select all {resources.length} visible in {summary.name}"
      title="Select all {resources.length} visible in {summary.name}"
    />
    <!-- The whole summary row is ONE explicit expand control: collapsed
         shows only the summary, expanded reveals the item rows. -->
    <button
      class="min-w-0 flex-1 rounded-lg text-left"
      onclick={() => onExpand(summary.provider)}
      aria-expanded={expanded}
      aria-label={expanded ? `Collapse ${summary.name}` : `Expand ${summary.name}`}
      data-testid="provider-toggle-{summary.provider}"
    >
      <span class="flex items-center gap-3">
        <span
          aria-hidden="true"
          class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg {meta.tile}"
        >
          <TileIcon size={20} class={meta.accent} />
        </span>
        <span class="min-w-0 flex-1">
          <span class="flex items-baseline justify-between gap-2">
            <span class="truncate text-[15px] font-semibold text-zinc-900 dark:text-zinc-100">
              {summary.name}
              <span class="ml-1 font-normal text-zinc-500">· {resources.length}</span>
              {#if staleCount > 0}
                <span class="ml-1 font-normal text-amber-600 dark:text-amber-400">
                  · {staleCount} stale
                </span>
              {/if}
            </span>
            <span class="tnum flex shrink-0 items-baseline gap-1.5">
              <span class="font-mono text-[15px] text-zinc-700 dark:text-zinc-300">
                {formatBytes(summary.reclaimable_bytes)}
              </span>
              <span class="font-mono text-xs text-zinc-400">{pct}</span>
            </span>
          </span>
          <span class="mt-1.5 block h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
            <span class="block h-full rounded-full {barColor}" style="width: {barPct}%"></span>
          </span>
          <span class="mt-1 block text-xs text-zinc-500">
            {resources.length}
            {resources.length === 1 ? "item" : "items"}
            {#if reviewCount > 0}
              · <span class="font-medium text-amber-600 dark:text-amber-400"
                >{reviewCount} need review</span
              >
            {:else}
              · Safe to clean
            {/if}
            {#if !expanded}
              <span class="text-zinc-400"> — expand to see items</span>
            {/if}
          </span>
        </span>
        <span aria-hidden="true" class="flex h-8 w-8 shrink-0 items-center justify-center text-zinc-400">
          <ChevronDown
            size={18}
            class="transition-transform duration-150 motion-reduce:transition-none {expanded
              ? ''
              : '-rotate-90'}"
          />
        </span>
      </span>
    </button>
    <button
      class="mt-1.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-zinc-200 text-zinc-500 hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
      onclick={() => onRescan(summary.provider)}
      disabled={rescanning}
      aria-label="Rescan {summary.name}"
      title="Rescan this provider"
    >
      <span aria-hidden="true" class="flex {rescanning ? 'animate-spin' : ''}">
        <RotateCw size={14} />
      </span>
    </button>
  </div>

  {#if expanded}
    <ul class="mt-1 divide-y divide-zinc-100 dark:divide-zinc-800">
      {#each resources as r (r.id)}
        <ResourceRow
          resource={r}
          checked={selected.includes(r.id)}
          {showPath}
          {onToggle}
          {onOpenDetail}
        />
      {/each}
    </ul>
  {/if}
</section>
