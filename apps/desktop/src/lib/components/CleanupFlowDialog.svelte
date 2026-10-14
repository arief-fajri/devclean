<script lang="ts">
  import { coverageNote, previewCoverage, previewPhase } from "$lib/cleanup-view";
  import { formatBytes, truncateMiddle } from "$lib/format";
  import { cleanupOutcome } from "$lib/cleanup-view";
  import { safetyMeta } from "$lib/safety";
  import type {
    CleanupPreview,
    CleanupPreviewItem,
    CleanupProgress,
    CleanupResult,
    ResourceImpact,
  } from "$lib/tauri";
  import { LoaderCircle, TriangleAlert, X } from "lucide-svelte";

  interface Props {
    /** Null while re-measuring, and when the re-measure failed. */
    preview: CleanupPreview | null;
    /** `preview_cleanup` is walking the selected paths right now. */
    previewing?: boolean;
    /** Why the re-measure failed — rendered inside the dialog. */
    previewError?: string | null;
    /** How many resources the user selected, for honest coverage wording. */
    selectedCount?: number;
    /** Re-measure the same selection after a failure. */
    onRetry?: () => void;
    /** Start a fresh scan, for a preview that resolved nothing. */
    onRescanFromPreview?: () => void;
    measuredAt: Date | null;
    showItems: boolean;
    cleaning: boolean;
    cancelling: boolean;
    progress: CleanupProgress | null;
    /** Set when the run finishes → the `done` phase. */
    results: CleanupResult[] | null;
    lookupImpact: (id: string) => ResourceImpact | undefined;
    /** Filesystem path of the resource — display-only (the backend resolves
     * deletion from the opaque id, never from this string). */
    lookupPath: (id: string) => string | undefined;
    nameOf: (id: string) => string;
    onConfirm: () => void;
    onCancel: () => void;
    onCancelCleanup: () => void;
    onRescan: () => void;
    onDone: () => void;
  }

  let {
    preview,
    previewing = false,
    previewError = null,
    selectedCount = 0,
    onRetry = () => {},
    onRescanFromPreview = () => {},
    measuredAt,
    showItems,
    cleaning,
    cancelling,
    progress,
    results,
    lookupImpact,
    lookupPath,
    nameOf,
    onConfirm,
    onCancel,
    onCancelCleanup,
    onRescan,
    onDone,
  }: Props = $props();

  // Which phase is showing is decided by state in `$lib/cleanup-view`, so the
  // dialog cannot render a review over a preview that never arrived.
  const phase = $derived(
    previewPhase({
      previewing,
      cleaning,
      hasResults: results !== null,
      previewFailed: previewError !== null,
    }),
  );
  /** What the preview actually resolved, and what to say about it. */
  const coverage = $derived(
    previewCoverage(preview?.items.length ?? 0, selectedCount || preview?.items.length || 0),
  );
  const coverageText = $derived(coverageNote(coverage));
  const nothingToClean = $derived(phase === "review" && coverage.kind === "empty");
  /** Nothing can be confirmed until a preview exists with something in it. */
  const canConfirm = $derived(preview !== null && coverage.kind !== "empty");

  // Explicit understanding gate (G-UI-01). Resets for every new preview.
  let acknowledged = $state(false);
  $effect(() => {
    void preview?.total_bytes;
    acknowledged = false;
  });

  const done = $derived(progress?.done ?? 0);
  const total = $derived(progress?.total ?? preview?.items.length ?? 0);
  const progressPct = $derived(total > 0 ? Math.round((done / total) * 100) : 0);

  const reviewItems = $derived((preview?.items ?? []).filter((i) => i.safety !== "safe"));
  const safeItems = $derived((preview?.items ?? []).filter((i) => i.safety === "safe"));

  const doneCounts = $derived.by(() => {
    let success = 0;
    let partial = 0;
    let failed = 0;
    let cancelled = 0;
    for (const r of results ?? []) {
      if (r.status === "success") success += 1;
      else if (r.status === "partial") partial += 1;
      else if (r.status === "failed") failed += 1;
      else cancelled += 1;
    }
    return { success, partial, failed, cancelled };
  });
  /** Honest split of freed-now vs pending-in-Trash (unit-tested pure logic). */
  const outcome = $derived(cleanupOutcome(results ?? []));
  const doneFreed = $derived(outcome.freed);
  const donePending = $derived(outcome.pending);
  const doneTrashed = $derived(outcome.trashed);
  const doneRequested = $derived((results ?? []).reduce((a, r) => a + r.requested_bytes, 0));

  function onWindowKey(e: KeyboardEvent) {
    // Never dismiss mid-run; `done` closes like `Done`.
    // Never dismiss mid-run; every other phase closes like its own button.
    if (e.key === "Escape" && phase !== "running") {
      if (phase === "done") onDone();
      else onCancel();
    }
  }

  function resultBadge(status: CleanupResult["status"]): string {
    return status === "success"
      ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200"
      : status === "partial"
        ? "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
        : status === "failed"
          ? "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200"
          : "bg-zinc-200 text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300";
  }

  function resultMark(status: CleanupResult["status"]): string {
    if (status === "success") return "✓";
    if (status === "partial") return "~";
    if (status === "failed") return "✗";
    return "○";
  }
</script>

    {#snippet itemRow(item: CleanupPreviewItem, highlight: boolean)}
      {@const meta = safetyMeta(item.safety)}
      {@const Icon = meta.icon}
      {@const impact = lookupImpact(item.resource_id)}
      {@const consequence = item.warning ?? impact?.consequence ?? meta.hint}
      <li
        class="rounded-lg border p-2 {highlight
          ? 'border-amber-300 bg-amber-50 dark:border-amber-800 dark:bg-amber-950/40'
          : 'border-zinc-100 dark:border-zinc-800'}"
      >
        <div class="flex items-start justify-between gap-2">
          <span class="min-w-0 flex-1">
            <span class="block truncate text-zinc-700 dark:text-zinc-300">
              {item.name}
            </span>
            {#if lookupPath(item.resource_id)}
              <span
                class="mt-0.5 block truncate font-mono text-[11px] text-zinc-400"
                title={lookupPath(item.resource_id)}
              >
                {truncateMiddle(lookupPath(item.resource_id) ?? "")}
              </span>
            {/if}
          </span>
          <span class="flex shrink-0 items-center gap-1.5">
            <span
              class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {meta.badge}"
            >
              <span aria-hidden="true" class="inline-flex"><Icon size={11} strokeWidth={2.5} /></span>
              {meta.label}
            </span>
            <span class="tnum font-mono text-xs text-zinc-500">{formatBytes(item.bytes)}</span>
          </span>
        </div>
        <p class="mt-1 text-xs text-zinc-500">
          {consequence}
          {#if !item.warning && impact?.cost}
            <span class="text-zinc-400"> · {impact.cost}</span>
          {/if}
        </p>
        {#if item.warning}
          <p class="mt-0.5 flex items-center gap-1 text-xs text-amber-700 dark:text-amber-300">
            <span aria-hidden="true" class="inline-flex"><TriangleAlert size={11} /></span>
            {item.warning}
          </p>
        {:else if impact?.command}
          <p class="mt-0.5 font-mono text-[11px] text-zinc-400">{impact.command}</p>
        {/if}
      </li>
    {/snippet}

<svelte:window onkeydown={onWindowKey} />

<div
  class="fixed inset-0 z-50 flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
  aria-label={phase === "review"
    ? "Confirm cleanup"
    : phase === "measuring"
      ? "Re-measuring selected resources"
      : phase === "error"
        ? "Could not prepare the cleanup"
        : phase === "running"
          ? "Cleaning in progress"
          : "Cleanup result"}
  tabindex={-1}
>
  <button
    class="absolute inset-0 cursor-default bg-black/50"
    aria-label="Close dialog"
    onclick={() => {
      if (phase === "done") onDone();
      else if (phase !== "running") onCancel();
    }}
    disabled={phase === "running"}
  ></button>
  <div
    class="relative flex max-h-[85vh] w-full max-w-md flex-col overflow-hidden rounded-xl bg-white shadow-xl dark:bg-zinc-900"
  >
    <!-- Sticky header (C5): phase title + X on the right. -->
    <header
      class="flex shrink-0 items-start gap-3 border-b border-zinc-100 px-5 py-4 dark:border-zinc-800"
    >
      <div class="min-w-0 flex-1">
        {#if phase === "review" && preview}
          <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            Clean {formatBytes(preview.total_bytes)} from {preview.items.length}
            {preview.items.length === 1 ? "resource" : "resources"}?
          </h2>
          <p class="mt-0.5 text-sm text-zinc-500">
            Re-measured
            {#if measuredAt}
              {measuredAt.toLocaleTimeString()}
            {/if}
            (scan numbers can go stale)
          </p>
        {:else if phase === "measuring"}
          <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            Re-measuring…
          </h2>
          <p class="mt-0.5 text-sm text-zinc-500">
            Checking what is actually on disk for {selectedCount}
            {selectedCount === 1 ? "resource" : "resources"}.
          </p>
        {:else if phase === "error"}
          <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            Could not prepare the cleanup
          </h2>
          <p class="mt-0.5 text-sm text-zinc-500">Nothing was deleted.</p>
        {:else if phase === "running"}
          <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">Cleaning…</h2>
        {:else}
          <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            Cleanup complete
          </h2>
        {/if}
      </div>
      <button
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-zinc-500 hover:bg-zinc-100 hover:text-zinc-900 disabled:opacity-40 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
        onclick={() => {
          if (phase === "done") onDone();
          else if (phase !== "running") onCancel();
        }}
        disabled={phase === "running"}
        aria-label="Close dialog"
      >
        <span aria-hidden="true" class="flex"><X size={17} /></span>
      </button>
    </header>

    <!-- Body + footer are emitted per phase: each phase owns a scroll area
         so its footer can stay pinned while its content scrolls. -->
    {#if phase === "measuring"}
      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4" aria-live="polite">
        <div class="flex items-center gap-3">
          <span aria-hidden="true" class="flex animate-spin text-emerald-600"
            ><LoaderCircle size={18} /></span
          >
          <p class="text-sm text-zinc-700 dark:text-zinc-300">
            Walking {selectedCount}
            {selectedCount === 1 ? "path" : "paths"} to get current sizes.
          </p>
        </div>
        <p class="mt-3 text-xs text-zinc-500">
          This is the re-measure step behind the confirmation (G-UI-01): sizes shown by a scan can
          be stale, so every selected path is measured again before you are asked to confirm.
          A large selection can take a while.
        </p>
        <p class="mt-2 text-xs text-zinc-500">Nothing has been deleted.</p>
      </div>
      <footer
        class="flex shrink-0 justify-end border-t border-zinc-100 px-5 py-3 dark:border-zinc-800"
      >
        <button
          class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancel}
        >
          Cancel
        </button>
      </footer>
    {:else if phase === "error"}
      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        <div
          class="flex items-start gap-2 rounded-lg bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950/40 dark:text-red-300"
          role="alert"
        >
          <span aria-hidden="true" class="mt-0.5 flex shrink-0"
            ><TriangleAlert size={15} /></span
          >
          <span class="min-w-0 break-words">{previewError}</span>
        </div>
        <p class="mt-3 text-xs text-zinc-500">
          A large selection can outlast the re-measure budget. Selecting fewer items, or rescanning
          first, usually fixes it.
        </p>
      </div>
      <footer
        class="flex shrink-0 justify-end gap-2 border-t border-zinc-100 px-5 py-3 dark:border-zinc-800"
      >
        <button
          class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancel}
        >
          Cancel
        </button>
        <button
          class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
          onclick={onRetry}
        >
          Try again
        </button>
      </footer>
    {:else if phase === "review" && preview && !nothingToClean}
      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {#if showItems}
        <div class="mt-3 max-h-64 space-y-3 overflow-auto text-sm">
          {#if reviewItems.length > 0}
            <div>
              <p class="mb-1.5 text-xs font-semibold tracking-wide text-amber-700 uppercase dark:text-amber-300">
                Needs review ({reviewItems.length})
              </p>
              <ul class="space-y-1.5">
                {#each reviewItems as item (item.resource_id)}
                  {@render itemRow(item, true)}
                {/each}
              </ul>
            </div>
          {/if}
          {#if safeItems.length > 0}
            <div>
              {#if reviewItems.length > 0}
                <p class="mb-1.5 text-xs font-semibold tracking-wide text-emerald-700 uppercase dark:text-emerald-300">
                  Safe ({safeItems.length})
                </p>
              {/if}
              <ul class="space-y-1.5">
                {#each safeItems as item (item.resource_id)}
                  {@render itemRow(item, false)}
                {/each}
              </ul>
            </div>
          {/if}
        </div>
      {:else}
        <p class="mt-3 rounded-lg bg-zinc-100 p-2 text-sm text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
          Item list hidden (show_preview OFF) — {formatBytes(preview.total_bytes)} for
          {preview.items.length} resources, re-measured just now.
        </p>
      {/if}


      {#if coverageText}
        <p
          class="mt-3 flex items-start gap-1.5 rounded-lg bg-amber-50 px-2.5 py-2 text-xs text-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
          role="note"
        >
          <span aria-hidden="true" class="mt-0.5 flex shrink-0"><TriangleAlert size={13} /></span>
          <span>{coverageText}</span>
        </p>
      {/if}

      <label class="mt-3 flex items-start gap-2 text-sm text-zinc-700 dark:text-zinc-300">
        <input
          type="checkbox"
          class="mt-0.5 h-4 w-4 accent-emerald-600"
          bind:checked={acknowledged}
          disabled={cleaning}
        />
        I understand this data will be deleted and can be recreated.
      </label>
      </div>

      <!-- Sticky footer -->
      <footer
        class="flex shrink-0 justify-end gap-2 border-t border-zinc-100 px-5 py-3 dark:border-zinc-800"
      >
        <button
          class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 autofocus dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancel}
        >
          Cancel
        </button>
        <button
          class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
          onclick={onConfirm}
          disabled={cleaning || !acknowledged || !canConfirm}
          title={acknowledged ? "" : "Check the understanding statement first"}
        >
          {cleaning ? "Cleaning…" : `Clean ${formatBytes(preview.total_bytes)}`}
        </button>
      </footer>
    {:else if phase === "review" && nothingToClean}
      <!-- The backend resolved none of the selected ids. Saying "0 B" here
           reads as a bug; the ids are simply unknown now. -->
      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        <div
          class="flex items-start gap-2 rounded-lg bg-amber-50 p-3 text-sm text-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
          role="note"
        >
          <span aria-hidden="true" class="mt-0.5 flex shrink-0"><TriangleAlert size={15} /></span>
          <span>{coverageText}</span>
        </div>
      </div>
      <footer
        class="flex shrink-0 justify-end gap-2 border-t border-zinc-100 px-5 py-3 dark:border-zinc-800"
      >
        <button
          class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancel}
        >
          Cancel
        </button>
        <button
          class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
          onclick={onRescanFromPreview}
        >
          Rescan
        </button>
      </footer>
    {:else if phase === "running"}
      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
        Cleaning…
      </h2>
      <p class="mt-1 text-sm text-zinc-500" role="status" aria-live="polite">
        {#if total > 0}
          Item {Math.min(done + 1, total)} of {total}
        {:else}
          Working…
        {/if}
      </p>
      {#if total > 0}
        <div class="mt-3 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
          <div
            class="h-full rounded-full bg-emerald-500 transition-[width] duration-200"
            style="width: {progressPct}%"
          ></div>
        </div>
        <p class="tnum mt-1 text-right font-mono text-xs text-zinc-500">{progressPct}%</p>
      {:else}
        <div class="mt-3 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
          <div class="h-full w-1/3 animate-pulse rounded-full bg-emerald-500"></div>
        </div>
      {/if}

      {#if preview && showItems}
        <ul class="mt-3 max-h-56 space-y-1.5 overflow-auto text-sm">
          {#each preview.items as item, i (item.resource_id)}
            <li
              class="flex items-center gap-2 rounded-lg border border-zinc-100 px-2 py-1.5 dark:border-zinc-800 {i ===
              done
                ? 'bg-emerald-50 dark:bg-emerald-950/40'
                : ''}"
            >
              <span aria-hidden="true" class="flex shrink-0">
                {#if i < done}
                  <span class="text-sm text-emerald-600 dark:text-emerald-400">✓</span>
                {:else if i === done}
                  <LoaderCircle size={14} class="animate-spin text-emerald-600 dark:text-emerald-400" />
                {:else}
                  <span class="text-sm text-zinc-300 dark:text-zinc-600">○</span>
                {/if}
              </span>
              <span class="min-w-0 flex-1">
                <span
                  class="block truncate {i < done
                    ? 'text-zinc-400 line-through'
                    : 'text-zinc-700 dark:text-zinc-300'}"
                >
                  {item.name}
                </span>
                {#if lookupPath(item.resource_id)}
                  <span
                    class="mt-0.5 block truncate font-mono text-[11px] text-zinc-400"
                    title={lookupPath(item.resource_id)}
                  >
                    {truncateMiddle(lookupPath(item.resource_id) ?? "")}
                  </span>
                {/if}
              </span>
              <span class="tnum shrink-0 font-mono text-xs text-zinc-500">
                {formatBytes(item.bytes)}
              </span>
            </li>
          {/each}
        </ul>
      {/if}

      </div>

      <!-- Sticky footer -->
      <footer
        class="flex shrink-0 items-center justify-between gap-2 border-t border-zinc-100 px-5 py-3 dark:border-zinc-800"
      >
        <span class="text-xs text-zinc-500" role="status">
          {cancelling ? "Cancelling…" : "The app stays responsive — you can wait or cancel the rest."}
        </span>
        <button
          class="shrink-0 rounded-lg border border-zinc-200 px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancelCleanup}
          disabled={cancelling}
        >
          {cancelling ? "Cancelling…" : "Cancel remaining"}
        </button>
      </footer>
    {:else if phase === "done" && results}
      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
        Cleanup complete
      </h2>
      <p class="mt-1 text-sm text-zinc-500" role="status">
        {#if outcome.headline === "trashed"}
          <span class="tnum font-mono font-semibold text-zinc-700 dark:text-zinc-200">
            {formatBytes(donePending)}
          </span>
          moved to Trash — space returns when you empty it
        {:else if outcome.headline === "mixed"}
          <span class="tnum font-mono font-semibold text-emerald-700 dark:text-emerald-300">
            {formatBytes(doneFreed)}
          </span>
          freed now · <span class="tnum font-mono">{formatBytes(donePending)}</span> in the Trash
        {:else}
          <span class="tnum font-mono font-semibold text-emerald-700 dark:text-emerald-300">
            {formatBytes(doneFreed)}
          </span>
          freed of {formatBytes(doneRequested)} requested
        {/if}
      </p>
      {#if doneTrashed > 0}
        <p
          class="mt-2 rounded-lg bg-zinc-100 px-2.5 py-1.5 text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
          data-testid="trash-note"
        >
          {doneTrashed}
          {doneTrashed === 1 ? "item is" : "items are"} recoverable from the Trash until you empty
          it.
        </p>
      {/if}
      <p class="mt-1 text-xs text-zinc-500">
        {doneCounts.success} success · {doneCounts.partial} partial · {doneCounts.failed} failed ·
        {doneCounts.cancelled} cancelled
      </p>
      <ul class="mt-3 max-h-64 space-y-1.5 overflow-auto text-sm">
        {#each results as r (r.resource_id)}
          <li class="rounded-lg border border-zinc-100 px-2 py-1.5 dark:border-zinc-800">
            <div class="flex items-center justify-between gap-2">
              <span class="min-w-0 flex-1">
                <span class="block truncate" title={nameOf(r.resource_id)}>
                  {resultMark(r.status)}
                  {nameOf(r.resource_id)}
                </span>
                {#if lookupPath(r.resource_id)}
                  <span
                    class="mt-0.5 block truncate font-mono text-[11px] text-zinc-400"
                    title={lookupPath(r.resource_id)}
                  >
                    {truncateMiddle(lookupPath(r.resource_id) ?? "")}
                  </span>
                {/if}
              </span>
              <span class="tnum shrink-0 font-mono text-xs text-zinc-500">
                {formatBytes(r.requested_bytes)} → {formatBytes(r.freed_bytes)}
              </span>
              <span
                class="shrink-0 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {resultBadge(
                  r.status,
                )}"
              >
                {r.status}
              </span>
            </div>
            {#if r.error}
              <p class="mt-0.5 text-xs text-red-700 dark:text-red-300">{r.error}</p>
              <p class="mt-0.5 text-xs text-zinc-500">
                Next step: retry from the app, or clean it manually and press Rescan to
                re-measure.
              </p>
            {/if}
          </li>
        {/each}
      </ul>
      {#if doneCounts.cancelled > 0}
        <p class="mt-2 text-sm text-zinc-500 dark:text-zinc-400">
          Cancelled: the in-flight item finished, the rest were skipped.
        </p>
      {/if}
      </div>

      <!-- Sticky footer -->
      <footer
        class="flex shrink-0 justify-end gap-2 border-t border-zinc-100 px-5 py-3 dark:border-zinc-800"
      >
        <button
          class="rounded-lg border border-zinc-200 px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onRescan}
        >
          Rescan
        </button>
        <button
          class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 autofocus"
          onclick={onDone}
        >
          Done
        </button>
      </footer>
    {/if}
  </div>
</div>
