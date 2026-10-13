<script lang="ts">
  import { formatBytes, truncateMiddle } from "$lib/format";
  import { safetyMeta } from "$lib/safety";
  import type { Resource } from "$lib/tauri";

  interface Props {
    resource: Resource;
    checked: boolean;
    showPath: boolean;
    onToggle: (id: string) => void;
    onOpenDetail: (id: string) => void;
  }

  let { resource: r, checked, showPath, onToggle, onOpenDetail }: Props = $props();

  const meta = $derived(safetyMeta(r.safety));
  const Icon = $derived(meta.icon);
  /** Consequence text: backend `impact` first, old `detail` string as
   * fallback (pre-v0.3.0 caches), safety hint as last resort. */
  const consequence = $derived(
    r.impact?.consequence ?? (r.detail && !r.detail.startsWith("project:") ? r.detail : null) ?? meta.hint,
  );
</script>

<li class="flex items-start gap-3 py-2">
  <input
    type="checkbox"
    class="mt-1 h-4 w-4 shrink-0 accent-emerald-600"
    checked={checked}
    onchange={() => onToggle(r.id)}
    aria-label="Select {r.name}"
  />
  <div class="min-w-0 flex-1">
    <div class="flex items-baseline justify-between gap-2">
      <button
        class="min-w-0 flex-1 truncate text-left text-sm text-zinc-700 hover:underline dark:text-zinc-300"
        title={r.path}
        onclick={() => onOpenDetail(r.id)}
      >
        {r.name}
        {#if r.detail}
          <span class="ml-1 text-xs font-normal text-zinc-400">{r.detail}</span>
        {/if}
      </button>
      <span class="tnum shrink-0 font-mono text-xs text-zinc-500">
        {formatBytes(r.reclaimable_bytes)}
      </span>
    </div>
    <div class="mt-1 flex items-center gap-1.5">
      <span
        class="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-semibold uppercase {meta.badge}"
      >
        <span aria-hidden="true" class="inline-flex"><Icon size={11} strokeWidth={2.5} /></span>
        {meta.label}
      </span>
      <span class="min-w-0 flex-1 truncate text-xs text-zinc-500" title={consequence}>
        {consequence}
      </span>
    </div>
    {#if showPath}
      <p
        class="mt-0.5 truncate font-mono text-[11px] text-zinc-400"
        title={r.path}
        aria-label="Location: {r.path}"
      >
        {truncateMiddle(r.path)}
      </p>
    {/if}
    {#if r.impact?.cost || r.impact?.command}
      <p class="mt-0.5 truncate text-xs text-zinc-400">
        {#if r.impact.cost}{r.impact.cost}{/if}
        {#if r.impact.cost && r.impact.command} · {/if}
        {#if r.impact.command}<code
            class="rounded bg-zinc-100 px-1 font-mono dark:bg-zinc-800">{r.impact.command}</code
          >{/if}
      </p>
    {/if}
  </div>
</li>
