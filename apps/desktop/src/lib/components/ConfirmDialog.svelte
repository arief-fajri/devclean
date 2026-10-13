<script lang="ts">
  import { formatBytes } from "$lib/format";
  import { safetyMeta } from "$lib/safety";
  import type {
    CleanupPreview,
    CleanupPreviewItem,
    CleanupProgress,
    ResourceImpact,
  } from "$lib/tauri";

  interface Props {
    preview: CleanupPreview;
    measuredAt: Date | null;
    showItems: boolean;
    cleaning: boolean;
    cancelling: boolean;
    progress: CleanupProgress | null;
    /** Consequence text by resource id (from the scan summary). */
    lookupImpact: (id: string) => ResourceImpact | undefined;
    onConfirm: () => void;
    onCancel: () => void;
    onCancelCleanup: () => void;
  }

  let {
    preview,
    measuredAt,
    showItems,
    cleaning,
    cancelling,
    progress,
    lookupImpact,
    onConfirm,
    onCancel,
    onCancelCleanup,
  }: Props = $props();

  // Explicit understanding gate (G-UI-01). Resets for every new preview.
  let acknowledged = $state(false);
  $effect(() => {
    void preview.total_bytes;
    acknowledged = false;
  });

  const progressPct = $derived(
    progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0,
  );
  /** Review items first — the user must read the risk before the bulk. */
  const reviewItems = $derived(preview.items.filter((i) => i.safety !== "safe"));
  const safeItems = $derived(preview.items.filter((i) => i.safety === "safe"));

  function onWindowKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !cleaning) onCancel();
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<div
  class="fixed inset-0 z-50 flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
  aria-label="Confirm cleanup"
  tabindex={-1}
>
  <button
    class="absolute inset-0 cursor-default bg-black/50"
    aria-label="Close dialog"
    onclick={() => {
      if (!cleaning) onCancel();
    }}
    disabled={cleaning}
  ></button>
  <div class="relative w-full max-w-md rounded-xl bg-white p-6 shadow-xl dark:bg-zinc-900">
    <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
      Clean {formatBytes(preview.total_bytes)} from {preview.items.length}
      {preview.items.length === 1 ? "resource" : "resources"}?
    </h2>
    <p class="mt-1 text-sm text-zinc-500">
      Re-measured
      {#if measuredAt}
        {measuredAt.toLocaleTimeString()}
      {/if}
      (scan numbers can go stale)
    </p>

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
        <div class="flex items-center justify-between gap-2">
          <span class="min-w-0 flex-1 truncate text-zinc-700 dark:text-zinc-300">
            {item.name}
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
          <p class="mt-0.5 text-xs text-amber-700 dark:text-amber-300">⚠ {item.warning}</p>
        {:else if impact?.command}
          <p class="mt-0.5 font-mono text-[11px] text-zinc-400">{impact.command}</p>
        {/if}
      </li>
    {/snippet}

    <label class="mt-3 flex items-start gap-2 text-sm text-zinc-700 dark:text-zinc-300">
      <input
        type="checkbox"
        class="mt-0.5 h-4 w-4 accent-emerald-600"
        bind:checked={acknowledged}
        disabled={cleaning}
      />
      I understand this data will be deleted and can be recreated.
    </label>

    <div class="mt-4 flex justify-end gap-2">
      {#if cleaning}
        <span class="mr-auto self-center text-xs text-zinc-500" role="status">
          {#if progress}
            Cleaning… {progress.done}/{progress.total}
          {:else}
            {cancelling ? "Cancelling…" : "Cleaning…"}
          {/if}
        </span>
      {/if}
      {#if cleaning}
        <button
          class="rounded-lg border border-zinc-200 px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancelCleanup}
          disabled={cancelling}
        >
          {cancelling ? "Cancelling…" : "Cancel remaining"}
        </button>
      {:else}
        <button
          class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 autofocus dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onCancel}
        >
          Cancel
        </button>
      {/if}
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
        onclick={onConfirm}
        disabled={cleaning || !acknowledged}
        title={acknowledged ? "" : "Check the understanding statement first"}
      >
        {cleaning ? "Cleaning…" : `Clean ${formatBytes(preview.total_bytes)}`}
      </button>
    </div>
    {#if cleaning && progress && progress.total > 0}
      <div class="mt-3 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
        <div class="h-full rounded-full bg-emerald-500" style="width: {progressPct}%"></div>
      </div>
    {/if}
  </div>
</div>
