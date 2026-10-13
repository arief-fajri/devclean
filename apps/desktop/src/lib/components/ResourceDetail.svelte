<script lang="ts">
  import { formatBytes } from "$lib/format";
  import { safetyMeta } from "$lib/safety";
  import type { Resource } from "$lib/tauri";
  import { Check, Copy } from "lucide-svelte";

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
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
  role="dialog"
  aria-modal="true"
  aria-label="Detail {resource.name}"
>
  <div class="w-full max-w-md rounded-xl bg-white p-6 shadow-xl dark:bg-zinc-900">
    <div class="flex items-start justify-between gap-2">
      <h2 class="min-w-0 flex-1 truncate text-lg font-semibold text-zinc-900 dark:text-zinc-100">
        {resource.name}
      </h2>
      <span
        class="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {meta.badge}"
      >
        <span aria-hidden="true" class="inline-flex"><Icon size={11} strokeWidth={2.5} /></span>
        {meta.label}
      </span>
    </div>

    <p class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">{consequence}</p>
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
      {#if resource.detail}
        <div class="flex justify-between gap-2">
          <dt class="text-zinc-500">Info</dt>
          <dd class="text-right">{resource.detail}</dd>
        </div>
      {/if}
    </dl>

    <div class="mt-5 flex justify-end gap-2">
      <button
        class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
        onclick={onClose}
      >
        Close
      </button>
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
        onclick={() => {
          onToggle(resource.id);
          onClose();
        }}
      >
        {selected ? "Deselect" : "Select for cleanup"}
      </button>
    </div>
  </div>
</div>
