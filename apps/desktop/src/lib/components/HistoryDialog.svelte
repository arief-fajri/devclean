<script lang="ts">
  import { formatBytes } from "$lib/format";
  import {
    filterHistory,
    groupHistory,
    historyTotals,
    when,
    type StatusFilter,
  } from "$lib/history";
  import type { HistoryEntry } from "$lib/tauri";
  import { CircleAlert, CircleCheck, CircleX, RefreshCw, Trash2 } from "lucide-svelte";

  interface Props {
    entries: HistoryEntry[];
    loading: boolean;
    error: string | null;
    onRefresh: () => void;
    onClose: () => void;
  }

  let { entries, loading, error, onRefresh, onClose }: Props = $props();

  let filter = $state<StatusFilter>("all");
  const visible = $derived(filterHistory(entries, filter));
  const days = $derived(groupHistory(visible));
  const totals = $derived(historyTotals(visible));

  const chip = (active: boolean) =>
    `rounded-full border px-2.5 py-1 text-xs font-medium ${
      active
        ? "border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900"
        : "border-zinc-200 text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
    }`;

  function statusIcon(status: HistoryEntry["status"]) {
    if (status === "success") return CircleCheck;
    if (status === "partial") return CircleAlert;
    if (status === "failed") return CircleX;
    return Trash2;
  }

  function statusColor(status: HistoryEntry["status"]): string {
    return status === "success"
      ? "text-emerald-600 dark:text-emerald-400"
      : status === "partial"
        ? "text-amber-600 dark:text-amber-400"
        : status === "failed"
          ? "text-red-600 dark:text-red-400"
          : "text-zinc-400";
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
  aria-label="Cleanup history"
  tabindex={-1}
>
  <button
    class="absolute inset-0 cursor-default bg-black/50"
    aria-label="Close dialog"
    onclick={onClose}
  ></button>
  <div
    class="relative max-h-[85vh] w-full max-w-lg overflow-auto rounded-xl bg-white p-6 shadow-xl dark:bg-zinc-900"
  >
    <div class="flex items-center justify-between gap-2">
      <div>
        <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">Cleanup history</h2>
        <p class="mt-0.5 text-xs text-zinc-500">
          Every cleanup is recorded per resource — what was requested and what was actually
          freed.
        </p>
      </div>
      <button
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-zinc-200 text-zinc-500 hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
        onclick={onRefresh}
        disabled={loading}
        aria-label="Refresh history"
        title="Refresh history"
      >
        <span aria-hidden="true" class="flex {loading ? 'animate-spin' : ''}">
          <RefreshCw size={14} />
        </span>
      </button>
    </div>

    {#if entries.length > 0}
      <div class="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <span class="tnum font-mono text-sm text-zinc-700 dark:text-zinc-300">
          {formatBytes(totals.freed)} freed
        </span>
        <span class="tnum font-mono text-xs text-zinc-400">
          of {formatBytes(totals.requested)} requested
        </span>
        {#if totals.problems > 0}
          <span class="text-xs text-amber-600 dark:text-amber-400">
            {totals.problems} need attention
          </span>
        {/if}
      </div>
      <div class="mt-2 flex flex-wrap gap-1.5" role="group" aria-label="Filter history">
        <button class={chip(filter === "all")} onclick={() => (filter = "all")} aria-pressed={filter === "all"}>
          All · {entries.length}
        </button>
        <button class={chip(filter === "success")} onclick={() => (filter = "success")} aria-pressed={filter === "success"}>
          Clean · {entries.filter((e) => e.status === "success" || e.status === "cancelled").length}
        </button>
        <button class={chip(filter === "problems")} onclick={() => (filter = "problems")} aria-pressed={filter === "problems"}>
          Problems · {totals.problems === entries.length ? totals.problems : entries.filter((e) => e.status === "partial" || e.status === "failed").length}
        </button>
      </div>
    {/if}

    {#if error}
      <p class="mt-3 rounded-lg bg-red-50 p-2 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
        {error}
      </p>
    {:else if visible.length === 0 && !loading}
      <p class="mt-4 text-sm text-zinc-500">
        {#if entries.length === 0}
          No cleanups recorded yet. Anything you clean will show up here.
        {:else}
          No entries match this filter.
        {/if}
      </p>
    {:else}
      {#each days as day (day.label)}
        <div class="mt-4">
          <div class="flex items-baseline justify-between gap-2">
            <h3 class="text-xs font-semibold tracking-wide text-zinc-500 uppercase">
              {day.label}
            </h3>
            <span class="tnum font-mono text-xs text-zinc-400">
              {formatBytes(day.freed)} freed
            </span>
          </div>
          <ul class="mt-1.5 divide-y divide-zinc-100 dark:divide-zinc-800">
            {#each day.entries as e (`${e.finished_at_unix}-${e.resource_id}`)}
              {@const Icon = statusIcon(e.status)}
              <li class="flex items-start gap-2 py-1.5">
                <span aria-hidden="true" class="mt-0.5 flex shrink-0 {statusColor(e.status)}">
                  <Icon size={14} />
                </span>
                <span class="min-w-0 flex-1">
                  <span class="flex items-baseline justify-between gap-2">
                    <span class="min-w-0 flex-1 truncate text-sm text-zinc-700 dark:text-zinc-300" title={e.name || e.resource_id}>
                      {e.name || e.resource_id}
                    </span>
                    <span class="tnum shrink-0 font-mono text-xs text-zinc-500">
                      {formatBytes(e.requested_bytes)} → {formatBytes(e.freed_bytes)}
                    </span>
                    <span
                      class="shrink-0 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {e.status ===
                      'success'
                        ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200'
                        : e.status === 'partial'
                          ? 'bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200'
                          : e.status === 'failed'
                            ? 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
                            : 'bg-zinc-200 text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300'}"
                    >
                      {e.status}
                    </span>
                  </span>
                  <span class="mt-0.5 block text-xs text-zinc-400">
                    {when(e.finished_at_unix)}{#if e.provider} · {e.provider}{/if}
                  </span>
                  {#if e.error}
                    <span class="mt-0.5 block text-xs text-red-700 dark:text-red-300">{e.error}</span>
                  {/if}
                </span>
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    {/if}

    <div class="mt-5 flex justify-end">
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
        onclick={onClose}
      >
        Close
      </button>
    </div>
  </div>
</div>