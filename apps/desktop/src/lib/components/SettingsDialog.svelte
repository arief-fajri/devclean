<script lang="ts">
  import { providerMeta } from "$lib/providerMeta";
  import { formatBytes } from "$lib/format";
  import type { ProviderInfo, Settings, ThemeMode } from "$lib/tauri";

  interface Props {
    settings: Settings;
    /** From `get_providers` — the backend registry is the source of truth. */
    providers: ProviderInfo[];
    /** Last scan results per provider id, for a live size hint. */
    providerStats: Record<string, { count: number; bytes: number }>;
    saving: boolean;
    error: string | null;
    onSave: (next: Settings) => void;
    onClose: () => void;
  }

  let { settings, providers, providerStats, saving, error, onSave, onClose }: Props = $props();

  // Local draft, seeded from props once when the dialog mounts
  // (the dialog always fresh-mounts via {#if}, so seed-once is correct).
  let roots = $state<string[]>([]);
  let newRoot = $state("");
  let excluded = $state<string[]>([]);
  let newExcluded = $state("");
  let disabledProviders = $state<string[]>([]);
  let requireConfirmation = $state(true);
  let showPreview = $state(true);
  let allowPrivileged = $state(false);
  let theme = $state<ThemeMode>("system");
  let seeded = $state(false);
  $effect.pre(() => {
    if (!seeded) {
      roots = [...settings.scan_roots];
      excluded = [...settings.excluded_paths];
      disabledProviders = [...(settings.disabled_providers ?? [])];
      requireConfirmation = settings.require_confirmation;
      showPreview = settings.show_preview;
      allowPrivileged = settings.allow_privileged_operations;
      theme = settings.theme ?? "system";
      seeded = true;
    }
  });

  function toggleProvider(id: string) {
    disabledProviders = disabledProviders.includes(id)
      ? disabledProviders.filter((p) => p !== id)
      : [...disabledProviders, id];
  }

  const activeCount = $derived(providers.filter((p) => !disabledProviders.includes(p.id)).length);

  function addRoot() {
    const p = newRoot.trim();
    if (p && !roots.includes(p)) roots = [...roots, p];
    newRoot = "";
  }

  function removeRoot(p: string) {
    roots = roots.filter((r) => r !== p);
  }

  function addExcluded() {
    const p = newExcluded.trim();
    if (p && !excluded.includes(p)) excluded = [...excluded, p];
    newExcluded = "";
  }

  function removeExcluded(p: string) {
    excluded = excluded.filter((r) => r !== p);
  }

  function save() {
    onSave({
      ...settings,
      scan_roots: roots,
      excluded_paths: excluded,
      disabled_providers: disabledProviders,
      require_confirmation: requireConfirmation,
      show_preview: showPreview,
      allow_privileged_operations: allowPrivileged,
      theme,
    });
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
  role="dialog"
  aria-modal="true"
  aria-label="Settings"
>
  <div class="max-h-[85vh] w-full max-w-md overflow-auto rounded-xl bg-white p-6 shadow-xl dark:bg-zinc-900">
    <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">Settings</h2>

    <h3 class="mt-4 text-sm font-semibold text-zinc-700 dark:text-zinc-300">
      Providers
      <span class="ml-1 font-normal text-zinc-500">
        {activeCount} of {providers.length} active
      </span>
    </h3>
    <p class="mt-1 text-xs text-zinc-500">
      Disabled providers are skipped during scans. Tools that are not installed simply
      report nothing.
    </p>
    {#if providers.length === 0}
      <p class="mt-2 text-sm text-zinc-500">Loading providers…</p>
    {:else}
      <ul class="mt-2 divide-y divide-zinc-100 dark:divide-zinc-800">
        {#each providers as p (p.id)}
          {@const meta = providerMeta(p.id)}
          {@const Icon = meta.icon}
          {@const stat = providerStats[p.id]}
          {@const isDisabled = disabledProviders.includes(p.id)}
          <li class="flex items-center gap-3 py-2">
            <span
              aria-hidden="true"
              class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg {meta.tile}"
            >
              <Icon size={16} class={meta.accent} />
            </span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm text-zinc-700 dark:text-zinc-300">
                {p.name}
              </span>
              <span class="block text-xs text-zinc-400">
                {#if stat && stat.count > 0}
                  {stat.count} {stat.count === 1 ? "item" : "items"} · {formatBytes(stat.bytes)}
                {:else if isDisabled}
                  Skipped during scans
                {:else}
                  Nothing found — tool may not be installed
                {/if}
              </span>
            </span>
            <label class="flex shrink-0 cursor-pointer items-center gap-1.5 text-xs text-zinc-500">
              <input
                type="checkbox"
                class="h-4 w-4 accent-emerald-600"
                checked={!isDisabled}
                onchange={() => toggleProvider(p.id)}
                aria-label="Scan {p.name}"
              />
              {isDisabled ? "Off" : "On"}
            </label>
          </li>
        {/each}
      </ul>
    {/if}

    <h3 class="mt-5 text-sm font-semibold text-zinc-700 dark:text-zinc-300">Scan roots</h3>
    <p class="mt-1 text-xs text-zinc-500">
      Folders to scan. Cleanup outside these roots is always rejected by the safety validator.
    </p>
    <ul class="mt-2 space-y-1.5 text-sm">
      {#each roots as r (r)}
        <li
          class="flex items-center justify-between gap-2 rounded-lg bg-zinc-100 px-2 py-1.5 font-mono text-xs dark:bg-zinc-800"
        >
          <span class="min-w-0 flex-1 truncate" title={r}>{r}</span>
          <button
            class="shrink-0 rounded px-2 py-0.5 text-xs text-red-600 hover:bg-red-50 dark:hover:bg-red-950"
            onclick={() => removeRoot(r)}
            aria-label="Remove {r}"
          >
            Remove
          </button>
        </li>
      {/each}
    </ul>
    <div class="mt-2 flex gap-2">
      <input
        type="text"
        class="min-w-0 flex-1 rounded-lg border border-zinc-200 bg-transparent px-2 py-1.5 font-mono text-xs dark:border-zinc-700"
        placeholder="/Users/you/Projects"
        bind:value={newRoot}
        onkeydown={(e) => {
          if (e.key === "Enter") addRoot();
        }}
        aria-label="New scan root path"
      />
      <button
        class="shrink-0 rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
        onclick={addRoot}
      >
        + Add
      </button>
    </div>

    <h3 class="mt-4 text-sm font-semibold text-zinc-700 dark:text-zinc-300">Ignore list</h3>
    <p class="mt-1 text-xs text-zinc-500">
      Projects skipped by the scan. Not a safety verdict — the validator still applies at cleanup.
    </p>
    <ul class="mt-2 space-y-1.5 text-sm">
      {#each excluded as x (x)}
        <li
          class="flex items-center justify-between gap-2 rounded-lg bg-zinc-100 px-2 py-1.5 font-mono text-xs dark:bg-zinc-800"
        >
          <span class="min-w-0 flex-1 truncate" title={x}>{x}</span>
          <button
            class="shrink-0 rounded px-2 py-0.5 text-xs text-red-600 hover:bg-red-50 dark:hover:bg-red-950"
            onclick={() => removeExcluded(x)}
            aria-label="Remove {x} from the ignore list"
          >
            Remove
          </button>
        </li>
      {/each}
    </ul>
    <div class="mt-2 flex gap-2">
      <input
        type="text"
        class="min-w-0 flex-1 rounded-lg border border-zinc-200 bg-transparent px-2 py-1.5 font-mono text-xs dark:border-zinc-700"
        placeholder="/Users/you/Projects/huge-monorepo"
        bind:value={newExcluded}
        onkeydown={(e) => {
          if (e.key === "Enter") addExcluded();
        }}
        aria-label="New ignored path"
      />
      <button
        class="shrink-0 rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
        onclick={addExcluded}
      >
        + Ignore
      </button>
    </div>

    <div class="mt-4 space-y-2 text-sm">
      <label class="flex items-center gap-2">
        <span class="text-zinc-700 dark:text-zinc-300">Theme</span>
        <select
          class="rounded-lg border border-zinc-200 bg-transparent px-2 py-1 text-sm dark:border-zinc-700"
          bind:value={theme}
          aria-label="Theme"
        >
          <option value="system">System</option>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>
      <label class="flex items-center gap-2">
        <input type="checkbox" class="h-4 w-4 accent-emerald-600" bind:checked={requireConfirmation} />
        Ask for confirmation before cleanup
      </label>
      <label class="flex items-center gap-2">
        <input type="checkbox" class="h-4 w-4 accent-emerald-600" bind:checked={showPreview} />
        Show the item list in the confirmation dialog
      </label>
      <label class="flex items-start gap-2">
        <input type="checkbox" class="mt-0.5 h-4 w-4 accent-red-600" bind:checked={allowPrivileged} />
        <span>
          Privileged operations
          <span class="block text-xs text-amber-700 dark:text-amber-300">
            Off by default. Only enable if you understand the risks.
          </span>
        </span>
      </label>
    </div>

    {#if error}
      <p class="mt-3 rounded-lg bg-red-50 p-2 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
        {error}
      </p>
    {/if}
    <p class="mt-3 text-xs text-zinc-400">Stored on disk, reloaded at startup.</p>

    <div class="mt-4 flex justify-end gap-2">
      <button
        class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
        onclick={onClose}
        disabled={saving}
      >
        Cancel
      </button>
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
        onclick={save}
        disabled={saving || roots.length === 0}
        title={roots.length === 0 ? "At least one scan root is required" : ""}
      >
        {saving ? "Saving…" : "Save"}
      </button>
    </div>
  </div>
</div>
