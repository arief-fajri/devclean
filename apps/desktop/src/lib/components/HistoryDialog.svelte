<script lang="ts">
  import { formatBytes } from "$lib/format";
  import {
    countRunsByFilter,
    entryCountLabel,
    filterRuns,
    groupRunsByDay,
    isLegacyRun,
    runHasProblems,
    runTotals,
    statusBadge,
    statusColor,
    when,
    type RunFilter,
  } from "$lib/history";
  import type { CleanupRun, HistoryEntry } from "$lib/tauri";
  import {
    ChevronDown,
    CircleAlert,
    CircleCheck,
    CircleX,
    RefreshCw,
    Trash2,
    X,
  } from "lucide-svelte";

  interface Props {
    runs: CleanupRun[];
    /** True when the backend may hold more runs than were loaded. */
    truncated: boolean;
    loading: boolean;
    error: string | null;
    onRefresh: () => void;
    onClose: () => void;
  }

  let { runs, truncated, loading, error, onRefresh, onClose }: Props = $props();

  let filter = $state<RunFilter>("all");
  /** Run ids whose per-resource rows are open. Empty = all collapsed, so a
   * 289-resource run never floods the dialog. */
  let openRuns = $state<string[]>([]);

  const visible = $derived(filterRuns(runs, filter));
  const days = $derived(groupRunsByDay(visible));
  const totals = $derived(runTotals(visible));

  const chip = (active: boolean) =>
    `rounded-full border px-2.5 py-1 text-xs font-medium ${
      active
        ? "border-zinc-900 bg-zinc-900 text-white dark:border-zinc-100 dark:bg-zinc-100 dark:text-zinc-900"
        : "border-zinc-200 text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
    }`;

  function runKey(r: CleanupRun): string {
    return r.run_id || `legacy-${r.finished_at_unix}`;
  }

  function toggleRun(r: CleanupRun) {
    const key = runKey(r);
    openRuns = openRuns.includes(key)
      ? openRuns.filter((k) => k !== key)
      : [...openRuns, key];
  }

  function statusIcon(status: HistoryEntry["status"]) {
    if (status === "success") return CircleCheck;
    if (status === "partial") return CircleAlert;
    if (status === "failed") return CircleX;
    return Trash2;
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
    class="relative flex max-h-[85vh] w-full max-w-lg flex-col overflow-hidden rounded-xl bg-white shadow-xl dark:bg-zinc-900"
  >
    <!-- Sticky header -->
    <header
      class="flex shrink-0 items-center gap-3 border-b border-zinc-100 px-5 py-4 dark:border-zinc-800"
    >
      <div class="min-w-0 flex-1">
        <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">Cleanup history</h2>
        <p class="mt-0.5 text-xs text-zinc-500">
          Every cleanup run, with what was requested and what was actually freed.
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
      <button
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-zinc-500 hover:bg-zinc-100 hover:text-zinc-900 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
        onclick={onClose}
        aria-label="Close history"
      >
        <span aria-hidden="true" class="flex"><X size={17} /></span>
      </button>
    </header>

    <!-- Scrollable body -->
    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {#if runs.length > 0}
        <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
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
        <div class="mt-2 flex flex-wrap gap-1.5" role="group" aria-label="Filter cleanups">
          <button class={chip(filter === "all")} onclick={() => (filter = "all")} aria-pressed={filter === "all"}>
            All · {countRunsByFilter(runs, "all")}
          </button>
          <button class={chip(filter === "clean")} onclick={() => (filter = "clean")} aria-pressed={filter === "clean"}>
            Clean · {countRunsByFilter(runs, "clean")}
          </button>
          <button class={chip(filter === "problems")} onclick={() => (filter = "problems")} aria-pressed={filter === "problems"}>
            Problems · {countRunsByFilter(runs, "problems")}
          </button>
        </div>
        {#if truncated}
          <p class="mt-2 text-[11px] text-zinc-400">
            Showing the {runs.length} most recent runs — older ones stay in the log.
          </p>
        {/if}
      {/if}

      {#if error}
        <p class="mt-3 rounded-lg bg-red-50 p-2 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
          {error}
        </p>
      {:else if visible.length === 0 && !loading}
        <p class="mt-4 text-sm text-zinc-500">
          {#if runs.length === 0}
            No cleanups recorded yet. Anything you clean will show up here.
          {:else}
            No cleanups match this filter.
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
                {day.runs.length}
                {day.runs.length === 1 ? " run" : " runs"} · {formatBytes(day.freed)} freed
              </span>
            </div>
            <ul class="mt-1.5 space-y-1.5">
              {#each day.runs as run (runKey(run))}
                {@const open = openRuns.includes(runKey(run))}
                <li class="rounded-lg border border-zinc-100 dark:border-zinc-800">
                  <button
                    class="flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-zinc-50 dark:hover:bg-zinc-800/50"
                    onclick={() => toggleRun(run)}
                    aria-expanded={open}
                    data-testid="history-run-{runKey(run)}"
                  >
                    <span
                      aria-hidden="true"
                      class="flex shrink-0 {runHasProblems(run)
                        ? 'text-amber-600 dark:text-amber-400'
                        : 'text-emerald-600 dark:text-emerald-400'}"
                    >
                      {#if runHasProblems(run)}
                        <CircleAlert size={15} />
                      {:else}
                        <CircleCheck size={15} />
                      {/if}
                    </span>
                    <span class="min-w-0 flex-1">
                      <span class="flex items-baseline justify-between gap-2">
                        <span class="min-w-0 truncate text-sm text-zinc-800 dark:text-zinc-200">
                          {when(run.finished_at_unix)}
                        </span>
                        <span class="tnum shrink-0 font-mono text-xs text-zinc-600 dark:text-zinc-300">
                          {formatBytes(run.requested_bytes)} → {formatBytes(run.freed_bytes)}
                        </span>
                      </span>
                      <span class="mt-0.5 block text-xs text-zinc-400">
                        {entryCountLabel(run.entry_count)}
                        · {run.success} success
                        {#if run.partial > 0}· {run.partial} partial{/if}
                        {#if run.failed > 0}· {run.failed} failed{/if}
                        {#if run.cancelled > 0}· {run.cancelled} cancelled{/if}
                        {#if run.providers.length > 0}
                          · {run.providers.join(", ")}
                        {/if}
                        {#if isLegacyRun(run)}
                          · recorded before run tracking
                        {/if}
                      </span>
                    </span>
                    <span aria-hidden="true" class="flex shrink-0 text-zinc-400">
                      <ChevronDown
                        size={15}
                        class="transition-transform duration-150 motion-reduce:transition-none {open
                          ? ''
                          : '-rotate-90'}"
                      />
                    </span>
                  </button>

                  {#if open}
                    <ul
                      class="max-h-64 divide-y divide-zinc-100 overflow-auto border-t border-zinc-100 dark:divide-zinc-800 dark:border-zinc-800"
                    >
                      {#each run.entries as e (`${e.resource_id}-${e.status}`)}
                        {@const Icon = statusIcon(e.status)}
                        <li class="flex items-start gap-2 px-3 py-1.5">
                          <span
                            aria-hidden="true"
                            class="mt-0.5 flex shrink-0 {statusColor(e.status)}"
                          >
                            <Icon size={13} />
                          </span>
                          <span class="min-w-0 flex-1">
                            <span class="flex items-baseline justify-between gap-2">
                              <span
                                class="min-w-0 flex-1 truncate text-xs text-zinc-700 dark:text-zinc-300"
                                title={e.name || e.resource_id}
                              >
                                {e.name || e.resource_id}
                              </span>
                              <span class="tnum shrink-0 font-mono text-[11px] text-zinc-500">
                                {formatBytes(e.requested_bytes)} → {formatBytes(e.freed_bytes)}
                              </span>
                              <span
                                class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase {statusBadge(
                                  e.status,
                                )}"
                              >
                                {e.status}
                              </span>
                            </span>
                            {#if e.error}
                              <span class="mt-0.5 block text-[11px] text-red-700 dark:text-red-300">
                                {e.error}
                              </span>
                            {/if}
                          </span>
                        </li>
                      {/each}
                    </ul>
                  {/if}
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      {/if}
    </div>

    <!-- Sticky footer -->
    <footer
      class="flex shrink-0 justify-end border-t border-zinc-100 px-5 py-3 dark:border-zinc-800"
    >
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
        onclick={onClose}
      >
        Close
      </button>
    </footer>
  </div>
</div>