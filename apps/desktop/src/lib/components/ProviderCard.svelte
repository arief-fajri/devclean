<script lang="ts">
  import { formatBytes } from "$lib/format";
  import type { ProviderSummary, Resource } from "$lib/tauri";

  interface Props {
    summary: ProviderSummary;
    resources: Resource[];
    selected: string[];
    maxBytes: number;
    onToggle: (id: string) => void;
    onToggleProvider: (provider: string) => void;
  }

  let { summary, resources, selected, maxBytes, onToggle, onToggleProvider }: Props = $props();

  const pct = $derived(
    maxBytes > 0 ? Math.max(3, Math.round((summary.reclaimable_bytes / maxBytes) * 100)) : 0,
  );
  const allChecked = $derived(
    resources.length > 0 && resources.every((r) => selected.includes(r.id)),
  );

  function badge(s: Resource["safety"]): string {
    return s === "safe"
      ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200"
      : s === "caution"
        ? "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
        : "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200";
  }
</script>

<section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
  <div class="flex items-center gap-3">
    <input
      type="checkbox"
      class="h-4 w-4 accent-emerald-600"
      checked={allChecked}
      onchange={() => onToggleProvider(summary.provider)}
      aria-label="Select all {summary.name}"
    />
    <div class="min-w-0 flex-1">
      <div class="flex items-baseline justify-between gap-2">
        <h2 class="truncate text-sm font-semibold text-zinc-900 dark:text-zinc-100">
          {summary.name}
          <span class="ml-1 font-normal text-zinc-500">· {summary.resource_count}</span>
        </h2>
        <span class="shrink-0 font-mono text-sm text-zinc-700 dark:text-zinc-300">
          {formatBytes(summary.reclaimable_bytes)}
        </span>
      </div>
      <div class="mt-2 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
        <div class="h-full rounded-full bg-emerald-500" style="width: {pct}%"></div>
      </div>
    </div>
  </div>

  {#if resources.length > 0}
    <ul class="mt-3 divide-y divide-zinc-100 text-sm dark:divide-zinc-800">
      {#each resources as r (r.id)}
        <li class="flex items-center gap-3 py-1.5">
          <input
            type="checkbox"
            class="h-4 w-4 accent-emerald-600"
            checked={selected.includes(r.id)}
            onchange={() => onToggle(r.id)}
            aria-label="Select {r.name}"
          />
          <span class="min-w-0 flex-1 truncate text-zinc-700 dark:text-zinc-300" title={r.path}>
            {r.name}
            {#if r.detail}
              <span class="ml-1 text-xs text-zinc-400">{r.detail}</span>
            {/if}
          </span>
          <span class="rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {badge(r.safety)}">
            {r.safety}
          </span>
          <span class="w-20 shrink-0 text-right font-mono text-xs text-zinc-500">
            {formatBytes(r.reclaimable_bytes)}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>
