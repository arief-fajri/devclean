<script lang="ts">
  import type { IconComponent } from "$lib/icons";

  interface Props {
    icon: IconComponent;
    value: string;
    label: string;
    iconClass: string;
    active?: boolean;
    onSelect: () => void;
  }

  let { icon: Icon, value, label, iconClass, active = false, onSelect }: Props = $props();
</script>

<button
  class="flex items-center gap-3 rounded-xl border border-zinc-200 bg-white px-4 py-3 text-left hover:bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:bg-zinc-800/60 {active
    ? 'ring-2 ring-emerald-600 ring-offset-1 dark:ring-offset-zinc-950'
    : ''}"
  onclick={onSelect}
  aria-pressed={active}
  title="Filter: {label}"
>
  <span aria-hidden="true" class="flex"><Icon size={20} class={iconClass} /></span>
  <span class="min-w-0">
    <span class="tnum block text-xl leading-6 font-semibold text-zinc-900 dark:text-zinc-100">
      {value}
    </span>
    <span class="block truncate text-xs text-zinc-500">{label}</span>
  </span>
</button>
