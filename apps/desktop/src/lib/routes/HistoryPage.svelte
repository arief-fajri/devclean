<script lang="ts">
  /**
   * History page — the old dialog minus the dialog. `ensureLoaded` runs on
   * entry, so opening the page always shows current data and returning to
   * Overview costs nothing.
   */
  import PageHeader from "$lib/components/PageHeader.svelte";
  import { formatBytes } from "$lib/format";
  import { historyStore } from "$lib/stores/history.svelte";
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
  } from "lucide-svelte";

  const runs = $derived(historyStore.runs);
  /** True when the backend may hold more runs than were loaded. */
  const truncated = $derived(historyStore.truncated);
  const loading = $derived(historyStore.loading);
  const error = $derived(historyStore.error);

  $effect(() => {
    void historyStore.ensureLoaded();
  });

  function onRefresh() {
    void historyStore.refresh();
  }

  let filter = $state<RunFilter>("all");
  /** Run ids whose per-resource rows are open. Empty = all collapsed, so a
   * 289-resource run never floods the page. */
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

<PageHeader title="Cleanup history" hint="Every cleanup run, with what was requested and what was actually freed">
  {#snippet actions()}
    <button
      class="flex h-9 w-9 items-center justify-center rounded-lg border border-zinc-200 text-zinc-600 hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
      onclick={onRefresh}
      disabled={loading}
      aria-label="Refresh history"
      title="Refresh history"
    >
      <span aria-hidden="true" class="flex {loading ? 'animate-spin' : ''}">
        <RefreshCw size={14} />
      </span>
    </button>
  {/snippet}
</PageHeader>

<div class="max-w-3xl">
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
                      {#if run.pre_allocated_sizing}
                        <span
                          class="ml-1 rounded bg-zinc-200 px-1 py-0.5 text-[10px] text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300"
                          title="Recorded before 0.1.0, when sizes summed file lengths instead of allocated blocks. Not comparable with newer runs."
                          data-testid="pre-allocated-sizing"
                        >
                          before 0.1.0
                        </span>
                      {/if}
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