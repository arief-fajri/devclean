<script lang="ts">
  import { formatBytes, isStaleSignal } from "$lib/format";
  import type { ProviderSummary, Resource } from "$lib/tauri";

  interface Props {
    summary: ProviderSummary;
    resources: Resource[];
    selected: string[];
    maxBytes: number;
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
    maxBytes,
    expanded,
    rescanning,
    onToggle,
    onToggleProvider,
    onExpand,
    onRescan,
    onOpenDetail,
  }: Props = $props();

  let query = $state("");
  let sort = $state<"largest" | "name">("largest");
  let staleOnly = $state(false);

  const pct = $derived(
    maxBytes > 0 ? Math.max(3, Math.round((summary.reclaimable_bytes / maxBytes) * 100)) : 0,
  );
  const allChecked = $derived(
    resources.length > 0 && resources.every((r) => selected.includes(r.id)),
  );
  const staleCount = $derived(resources.filter((r) => isStaleSignal(r.detail)).length);

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let list = resources.filter((r) => {
      if (staleOnly && !isStaleSignal(r.detail)) return false;
      if (q && !(r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q)))
        return false;
      return true;
    });
    if (sort === "largest") list = [...list].sort((a, b) => b.reclaimable_bytes - a.reclaimable_bytes);
    else if (sort === "name") list = [...list].sort((a, b) => a.name.localeCompare(b.name));
    return list;
  });

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
    <button
      class="text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200"
      onclick={() => onExpand(summary.provider)}
      aria-label={expanded ? `Collapse ${summary.name}` : `Expand ${summary.name}`}
      aria-expanded={expanded}
    >
      {expanded ? "▾" : "▸"}
    </button>
    <button class="min-w-0 flex-1 text-left" onclick={() => onExpand(summary.provider)}>
      <div class="flex items-baseline justify-between gap-2">
        <h2 class="truncate text-sm font-semibold text-zinc-900 dark:text-zinc-100">
          {summary.name}
          <span class="ml-1 font-normal text-zinc-500">· {summary.resource_count}</span>
          {#if staleCount > 0}
            <span class="ml-1 font-normal text-amber-600 dark:text-amber-400">
              · {staleCount} stale
            </span>
          {/if}
        </h2>
        <span class="shrink-0 font-mono text-sm text-zinc-700 dark:text-zinc-300">
          {formatBytes(summary.reclaimable_bytes)}
        </span>
      </div>
      <div class="mt-2 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
        <div class="h-full rounded-full bg-emerald-500" style="width: {pct}%"></div>
      </div>
    </button>
    <button
      class="shrink-0 rounded-lg border border-zinc-200 px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
      onclick={() => onRescan(summary.provider)}
      disabled={rescanning}
      aria-label="Rescan {summary.name}"
      title="Rescan this provider"
    >
      {rescanning ? "…" : "🔁"}
    </button>
  </div>

  {#if expanded}
    <div class="mt-3 flex flex-wrap items-center gap-2">
      <input
        type="search"
        class="min-w-0 flex-1 rounded-lg border border-zinc-200 bg-transparent px-2 py-1 text-sm dark:border-zinc-700"
        placeholder="Cari project…"
        bind:value={query}
        aria-label="Cari di {summary.name}"
      />
      <select
        class="rounded-lg border border-zinc-200 bg-transparent px-2 py-1 text-sm dark:border-zinc-700"
        bind:value={sort}
        aria-label="Urutkan"
      >
        <option value="largest">Terbesar</option>
        <option value="name">Nama</option>
      </select>
      <label
        class="flex items-center gap-1 text-xs text-zinc-500"
        title="Sinyal umur >90 hari — bukan vonis aman, validator tetap memutuskan"
      >
        <input type="checkbox" class="accent-amber-600" bind:checked={staleOnly} />
        Hanya stale (&gt;90d, sinyal)
      </label>
    </div>
    {#if visible.length > 0}
      <ul class="mt-2 divide-y divide-zinc-100 text-sm dark:divide-zinc-800">
        {#each visible as r (r.id)}
          <li class="flex items-center gap-3 py-1.5">
            <input
              type="checkbox"
              class="h-4 w-4 accent-emerald-600"
              checked={selected.includes(r.id)}
              onchange={() => onToggle(r.id)}
              aria-label="Select {r.name}"
            />
            <button
              class="min-w-0 flex-1 truncate text-left text-zinc-700 hover:underline dark:text-zinc-300"
              title={r.path}
              onclick={() => onOpenDetail(r.id)}
            >
              {r.name}
              {#if r.detail}
                <span class="ml-1 text-xs text-zinc-400">{r.detail}</span>
              {/if}
              {#if isStaleSignal(r.detail)}
                <span class="ml-1 rounded bg-amber-100 px-1 text-[10px] font-semibold text-amber-800 dark:bg-amber-900 dark:text-amber-200">
                  STALE?
                </span>
              {/if}
            </button>
            <span class="rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {badge(r.safety)}">
              {r.safety}
            </span>
            <span class="w-20 shrink-0 text-right font-mono text-xs text-zinc-500">
              {formatBytes(r.reclaimable_bytes)}
            </span>
          </li>
        {/each}
      </ul>
    {:else if resources.length > 0}
      <p class="mt-3 text-sm text-zinc-500">Tidak cocok dengan filter.</p>
    {/if}
  {:else if resources.length > 0}
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
