<script lang="ts">
  import { formatBytes } from "$lib/format";
  import type { Resource } from "$lib/tauri";

  interface Props {
    resource: Resource;
    selected: boolean;
    onToggle: (id: string) => void;
    onClose: () => void;
  }

  let { resource, selected, onToggle, onClose }: Props = $props();

  function badge(s: Resource["safety"]): string {
    return s === "safe"
      ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200"
      : s === "caution"
        ? "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
        : "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200";
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
      <h2 class="truncate text-lg font-semibold text-zinc-900 dark:text-zinc-100">
        {resource.name}
      </h2>
      <span class="rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {badge(resource.safety)}">
        {resource.safety}
      </span>
    </div>

    <dl class="mt-4 space-y-2 text-sm">
      <div class="flex justify-between gap-2">
        <dt class="text-zinc-500">Size</dt>
        <dd class="font-mono">{formatBytes(resource.size_bytes)}</dd>
      </div>
      <div class="flex justify-between gap-2">
        <dt class="text-zinc-500">Reclaimable</dt>
        <dd class="font-mono">{formatBytes(resource.reclaimable_bytes)}</dd>
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

    <p class="mt-3 break-all rounded-lg bg-zinc-100 p-2 font-mono text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
      {resource.path}
    </p>
    <p class="mt-2 text-xs text-zinc-400">
      Path hanya info tampilan — cleanup memakai id opaque ({resource.id}).
    </p>

    <div class="mt-5 flex justify-end gap-2">
      <button
        class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
        onclick={onClose}
      >
        Tutup
      </button>
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700"
        onclick={() => {
          onToggle(resource.id);
          onClose();
        }}
      >
        {selected ? "Batal pilih" : "Pilih untuk cleanup"}
      </button>
    </div>
  </div>
</div>
