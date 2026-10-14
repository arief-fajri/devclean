<script lang="ts">
  import { formatBytes, truncateMiddle } from "$lib/format";
  import { safetyMeta } from "$lib/safety";
  import type { Resource } from "$lib/tauri";
  import { Lock } from "lucide-svelte";

  interface Props {
    resource: Resource;
    checked: boolean;
    showPath: boolean;
    /** Held back for any reason — shown, but not selectable. */
    locked?: boolean;
    /** Why it is held back, shown as the tooltip. */
    lockedDetail?: string | null;
    /** Short badge text, e.g. "report only". */
    lockedLabel?: string | null;
    /** A scan is running; selection is unavailable. */
    disabled?: boolean;
    onToggle: (id: string) => void;
    onOpenDetail: (id: string) => void;
  }

  let {
    resource: r,
    checked,
    showPath,
    locked = false,
    lockedDetail = null,
    lockedLabel = null,
    disabled = false,
    onToggle,
    onOpenDetail,
  }: Props = $props();

  const unselectable = $derived(locked || disabled);

  const meta = $derived(safetyMeta(r.safety));
  const Icon = $derived(meta.icon);
  /** Consequence text: backend `impact` first, old `detail` string as
   * fallback (pre-v0.3.0 caches), safety hint as last resort. */
  const consequence = $derived(
    r.impact?.consequence ?? (r.detail && !r.detail.startsWith("project:") ? r.detail : null) ?? meta.hint,
  );
</script>

<li class="flex items-start gap-3 py-2 {locked ? 'opacity-60' : ''}">
  <input
    type="checkbox"
    class="mt-1 h-4 w-4 shrink-0 accent-emerald-600 disabled:opacity-40"
    checked={checked}
    disabled={unselectable}
    onchange={() => onToggle(r.id)}
    aria-label={locked ? `${r.name} cannot be cleaned` : `Select ${r.name}`}
  />
  <div class="min-w-0 flex-1">
    <div class="flex items-baseline justify-between gap-2">
      <button
        class="min-w-0 flex-1 truncate text-left text-sm text-zinc-700 hover:underline dark:text-zinc-300"
        title={r.path}
        onclick={() => onOpenDetail(r.id)}
      >
        {r.name}
        {#if locked}
          <span
            class="ml-1 inline-flex items-center gap-1 text-[11px] font-normal text-zinc-400"
            title={lockedDetail ??
              "The backend refuses this even if its id is sent directly."}
            data-testid="blocked-badge"
          >
            <span aria-hidden="true" class="flex"><Lock size={10} /></span>
            {lockedLabel ?? "kept"}
          </span>
        {:else if r.detail}
          <span class="ml-1 text-xs font-normal text-zinc-400">{r.detail}</span>
        {/if}
      </button>
      <span class="tnum shrink-0 font-mono text-xs text-zinc-500">
        {formatBytes(r.reclaimable_bytes)}
        {#if r.apparent_bytes}
          <span
            class="text-zinc-400"
            title="This file occupies far less on disk than its logical size — it is sparse. Deleting it frees the first figure."
          >
            (appears as {formatBytes(r.apparent_bytes)})
          </span>
        {/if}
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
