<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    hint?: string;
    /** Page-level actions (scan, refresh, save). */
    actions?: Snippet;
  }

  let { title, hint = "", actions }: Props = $props();
</script>

<header class="flex flex-wrap items-start justify-between gap-3 pt-6 pb-4">
  <div class="min-w-0">
    <h1 class="text-2xl font-bold tracking-tight text-zinc-900 dark:text-zinc-50">{title}</h1>
    {#if hint}
      <p class="mt-0.5 text-sm text-zinc-500">{hint}</p>
    {/if}
  </div>
  {#if actions}
    <div class="flex shrink-0 items-center gap-2">{@render actions()}</div>
  {/if}
</header>
