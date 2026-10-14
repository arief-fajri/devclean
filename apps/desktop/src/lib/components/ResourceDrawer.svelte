<script lang="ts">
  /**
   * Resource drill-down, as a non-modal right-hand drawer: the list stays
   * visible behind it, because the question "why is this 12 GB?" is always
   * asked about a row the user is looking at.
   *
   * Non-modal means non-blocking — but it still owns the keyboard while it is
   * open (Esc closes, Tab stays inside, focus returns to the opener), so it
   * is not a keyboard trap.
   */
  import { formatBytes } from "$lib/format";
  import { safetyMeta } from "$lib/safety";
  import { BLOCK_REASON_HINTS, BLOCK_REASON_LABELS, type Resource } from "$lib/tauri";
  import { Check, Copy, Lock, X } from "lucide-svelte";

  interface Props {
    resource: Resource;
    selected: boolean;
    onToggle: (id: string) => void;
    onClose: () => void;
  }

  let { resource, selected, onToggle, onClose }: Props = $props();

  const meta = $derived(safetyMeta(resource.safety));
  const Icon = $derived(meta.icon);
  const consequence = $derived(resource.impact?.consequence ?? meta.hint);

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  let panel = $state<HTMLElement | null>(null);
  let opener: HTMLElement | null = null;

  $effect(() => {
    opener = (document.activeElement as HTMLElement | null) ?? null;
    panel?.focus();
    return () => {
      if (copyTimer) clearTimeout(copyTimer);
      opener?.focus?.();
    };
  });

  function copyPath() {
    const done = () => {
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
      }, 1500);
    };
    const text = resource.path;
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(text).then(done, done);
    } else {
      done();
    }
  }

  const FOCUSABLE =
    'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab" || !panel) return;
    const items = [...panel.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
      (el) => el.offsetParent !== null,
    );
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  bind:this={panel}
  class="fixed inset-y-0 right-0 z-40 flex w-full max-w-[420px] animate-rise flex-col border-l border-zinc-200 bg-white shadow-2xl motion-reduce:animate-none dark:border-zinc-800 dark:bg-zinc-900"
  role="dialog"
  aria-modal="false"
  aria-label="Detail {resource.name}"
  tabindex={-1}
>
  <header
    class="flex shrink-0 items-start gap-3 border-b border-zinc-100 px-5 py-4 dark:border-zinc-800"
  >
    <h2 class="min-w-0 flex-1 truncate text-lg font-semibold text-zinc-900 dark:text-zinc-100">
      {resource.name}
    </h2>
    <span
      class="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {meta.badge}"
    >
      <span aria-hidden="true" class="inline-flex"><Icon size={11} strokeWidth={2.5} /></span>
      {meta.label}
    </span>
    <button
      class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-zinc-500 hover:bg-zinc-100 hover:text-zinc-900 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
      onclick={onClose}
      aria-label="Close detail"
    >
      <span aria-hidden="true" class="flex"><X size={17} /></span>
    </button>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
    <p class="text-sm text-zinc-600 dark:text-zinc-300">{consequence}</p>
    {#if resource.impact?.cost || resource.impact?.command}
      <p class="mt-1 text-xs text-zinc-500">
        {#if resource.impact.cost}{resource.impact.cost}{/if}
        {#if resource.impact.cost && resource.impact.command} · {/if}
        {#if resource.impact.command}<code
            class="rounded bg-zinc-100 px-1 font-mono dark:bg-zinc-800"
            >{resource.impact.command}</code
          >{/if}
      </p>
    {/if}

    <div class="mt-4">
      <div class="flex items-center justify-between gap-2">
        <p class="text-xs font-semibold tracking-wide text-zinc-400 uppercase">Will be removed</p>
        <button
          class="inline-flex items-center gap-1 rounded-lg border border-zinc-200 px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
          onclick={copyPath}
          aria-label="Copy path"
        >
          <span aria-hidden="true" class="flex">
            {#if copied}<Check size={12} />{:else}<Copy size={12} />{/if}
          </span>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <p
        class="mt-1.5 break-all rounded-lg bg-zinc-100 p-2 font-mono text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
      >
        {resource.path}
      </p>
      <p class="mt-1.5 text-xs text-zinc-400">
        Display-only — cleanup uses the opaque id ({resource.id}).
      </p>
    </div>

    <dl class="mt-4 space-y-2 text-sm">
      <div class="flex justify-between gap-2">
        <dt class="text-zinc-500">Size</dt>
        <dd class="tnum font-mono">{formatBytes(resource.size_bytes)}</dd>
      </div>
      <div class="flex justify-between gap-2">
        <dt class="text-zinc-500">Reclaimable</dt>
        <dd class="tnum font-mono">{formatBytes(resource.reclaimable_bytes)}</dd>
      </div>
      <div class="flex justify-between gap-2">
        <dt class="text-zinc-500">Provider</dt>
        <dd>{resource.provider}</dd>
      </div>
      {#if resource.age_days !== null && resource.age_days !== undefined}
        <div class="flex justify-between gap-2">
          <dt class="text-zinc-500">Age</dt>
          <dd class="tnum text-right font-mono">{resource.age_days}d</dd>
        </div>
      {/if}
      {#if resource.detail}
        <div class="flex justify-between gap-2">
          <dt class="text-zinc-500">Info</dt>
          <dd class="text-right">{resource.detail}</dd>
        </div>
      {/if}
      {#if resource.blocked_reason}
        <div class="flex justify-between gap-2">
          <dt class="text-zinc-500">Cannot be cleaned</dt>
          <dd class="inline-flex items-center gap-1 text-right text-zinc-500">
            <span aria-hidden="true" class="inline-flex"><Lock size={11} /></span>
            {BLOCK_REASON_LABELS[resource.blocked_reason]}
          </dd>
        </div>
      {/if}
    </dl>
    {#if resource.blocked_reason}
      <p
        class="mt-3 rounded-lg bg-zinc-100 px-2.5 py-2 text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
        data-testid="blocked-note"
      >
        {resource.blocked_detail ?? BLOCK_REASON_HINTS[resource.blocked_reason]}
      </p>
    {/if}
  </div>

  <footer
    class="flex shrink-0 justify-end gap-2 border-t border-zinc-100 px-5 py-3 dark:border-zinc-800"
  >
    <button
      class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
      onclick={onClose}
    >
      Close
    </button>
    <button
      class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
      onclick={() => {
        onToggle(resource.id);
        onClose();
      }}
      disabled={resource.blocked_reason != null}
      title={resource.blocked_detail ?? ""}
    >
      {selected ? "Deselect" : "Select for cleanup"}
    </button>
  </footer>
</div>