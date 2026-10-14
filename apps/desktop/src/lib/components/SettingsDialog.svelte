<script lang="ts">
  import { formatBytes } from "$lib/format";
  import { providerMeta } from "$lib/providerMeta";
  import { settingsStore } from "$lib/stores/settings.svelte";
  import { addableCandidates, alreadyAddedCount } from "$lib/view";
  import {
    DOCKER_SCOPE_CONSEQUENCE,
    DOCKER_SCOPE_LABELS,
    DOCKER_SCOPE_NEEDS_PRIVILEGED,
    type DockerPruneScope,
    type ProviderInfo,
    type ScanRootCandidate,
    type Settings,
    type ThemeMode,
  } from "$lib/tauri";
  import { Compass, Info, Plus, RotateCcw, ShieldAlert, X } from "lucide-svelte";

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

  type Tab = "scan" | "cleanup" | "appearance" | "advanced";
  let tab = $state<Tab>("scan");

  // Draft state. Seeded once from props (the dialog always fresh-mounts via
  // {#if}), and it survives tab switches so moving between tabs never
  // discards half-finished edits.
  let roots = $state<string[]>([]);
  let newRoot = $state("");
  let excluded = $state<string[]>([]);
  let newExcluded = $state("");
  let disabledProviders = $state<string[]>([]);
  let requireConfirmation = $state(true);
  let showPreview = $state(true);
  let allowPrivileged = $state(false);
  let theme = $state<ThemeMode>("system");
  let retentionDays = $state<number | null>(null);
  let dockerScope = $state<DockerPruneScope>("build-cache");
  let sqliteBackup = $state(false);
  let scanAutomatically = $state(false);
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
      retentionDays = settings.retention_days ?? null;
      dockerScope = settings.docker_prune_scope ?? "build-cache";
      sqliteBackup = settings.sqlite_backup ?? false;
      scanAutomatically = settings.scan_automatically ?? false;
      seeded = true;
      void settingsStore.refreshCandidates();
    }
  });

  const TABS: { id: Tab; label: string; hint: string }[] = [
    { id: "scan", label: "Scan", hint: "What gets looked at, and what is skipped" },
    { id: "cleanup", label: "Cleanup", hint: "What may be deleted, and when" },
    { id: "appearance", label: "Appearance", hint: "Theme and row display" },
    { id: "advanced", label: "Advanced", hint: "Resets and details" },
  ];

  const activeCount = $derived(providers.filter((p) => !disabledProviders.includes(p.id)).length);
  /** Suggestions only: paths already configured are not listed twice. */
  const addable = $derived(addableCandidates(settingsStore.candidates, roots));
  const alreadyAdded = $derived(alreadyAddedCount(settingsStore.candidates, roots));
  const scopeLabel = $derived(DOCKER_SCOPE_LABELS[dockerScope]);
  const scopeConsequence = $derived(DOCKER_SCOPE_CONSEQUENCE[dockerScope]);
  const scopeNeedsPrivileged = $derived(DOCKER_SCOPE_NEEDS_PRIVILEGED[dockerScope]);
  /** The scope is only meaningful if the privileged gate is also on. */
  const scopeEffective = $derived(
    scopeNeedsPrivileged && !allowPrivileged ? "build-cache" : dockerScope,
  );
  const dirty = $derived(
    JSON.stringify(roots) !== JSON.stringify(settings.scan_roots) ||
      JSON.stringify(excluded) !== JSON.stringify(settings.excluded_paths) ||
      JSON.stringify([...disabledProviders].sort()) !==
        JSON.stringify([...(settings.disabled_providers ?? [])].sort()) ||
      requireConfirmation !== settings.require_confirmation ||
      showPreview !== settings.show_preview ||
      allowPrivileged !== settings.allow_privileged_operations ||
      theme !== (settings.theme ?? "system") ||
      retentionDays !== (settings.retention_days ?? null) ||
      dockerScope !== (settings.docker_prune_scope ?? "build-cache") ||
      sqliteBackup !== (settings.sqlite_backup ?? false) ||
      scanAutomatically !== (settings.scan_automatically ?? false),
  );

  function toggleProvider(id: string) {
    disabledProviders = disabledProviders.includes(id)
      ? disabledProviders.filter((p) => p !== id)
      : [...disabledProviders, id];
  }

  function addRoot(p: string) {
    const v = p.trim();
    if (v && !roots.includes(v)) roots = [...roots, v];
    newRoot = "";
  }
  function removeRoot(p: string) {
    roots = roots.filter((r) => r !== p);
  }
  function addExcluded(p: string) {
    const v = p.trim();
    if (v && !excluded.includes(v)) excluded = [...excluded, v];
    newExcluded = "";
  }
  function removeExcluded(p: string) {
    excluded = excluded.filter((x) => x !== p);
  }

  function addCandidate(c: ScanRootCandidate) {
    if (!roots.includes(c.path)) roots = [...roots, c.path];
  }

  function resetAll() {
    roots = [];
    excluded = [];
    disabledProviders = [];
    requireConfirmation = true;
    showPreview = true;
    allowPrivileged = false;
    theme = "system";
    retentionDays = null;
    dockerScope = "build-cache";
    sqliteBackup = false;
    scanAutomatically = false;
    newRoot = "";
    newExcluded = "";
    void settingsStore.refreshCandidates();
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
      scan_automatically: scanAutomatically,
      theme,
      retention_days: retentionDays,
      docker_prune_scope: dockerScope,
      sqlite_backup: sqliteBackup,
    });
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && !saving) onClose();
  }}
/>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
  role="dialog"
  aria-modal="true"
  aria-label="Settings"
>
  <div
    class="flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl bg-white shadow-xl dark:bg-zinc-900"
  >
    <!-- Sticky header (C5): title + X on the right, stays put while the
         long body scrolls. -->
    <header
      class="flex shrink-0 items-center gap-3 border-b border-zinc-200 px-5 py-4 dark:border-zinc-800"
    >
      <div class="min-w-0 flex-1">
        <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">Settings</h2>
        <p class="truncate text-xs text-zinc-500">{TABS.find((t) => t.id === tab)?.hint}</p>
      </div>
      <button
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-zinc-500 hover:bg-zinc-100 hover:text-zinc-900 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
        onclick={onClose}
        disabled={saving}
        aria-label="Close settings"
      >
        <span aria-hidden="true" class="flex"><X size={17} /></span>
      </button>
    </header>

    <!-- Tabs -->
    <div
      class="flex shrink-0 gap-1 border-b border-zinc-200 px-3 dark:border-zinc-800"
      role="tablist"
      aria-label="Settings sections"
    >
      {#each TABS as t (t.id)}
        <button
          role="tab"
          aria-selected={tab === t.id}
          aria-controls="settings-panel-{t.id}"
          data-testid="settings-tab-{t.id}"
          class="relative -mb-px border-b-2 px-3 py-2.5 text-sm font-medium transition-colors {tab ===
          t.id
            ? 'border-emerald-600 text-zinc-900 dark:text-zinc-100'
            : 'border-transparent text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200'}"
          onclick={() => (tab = t.id)}
        >
          {t.label}
        </button>
      {/each}
    </div>

    <!-- Body: scrollable independently of the header and the footer. -->
    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {#if tab === "scan"}
        <div id="settings-panel-scan" role="tabpanel" aria-label="Scan settings">
          <h3 class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">Providers</h3>
          <p class="mt-1 text-xs text-zinc-500">
            A disabled provider is skipped entirely — it never runs and never appears in the
            summary. A tool that is not installed simply reports nothing.
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
                <li class="flex items-center gap-3 py-2.5">
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
                        {stat.count}
                        {stat.count === 1 ? "item" : "items"} · {formatBytes(stat.bytes)} found
                      {:else if isDisabled}
                        Skipped — not scanned
                      {:else}
                        Nothing found — the tool may not be installed
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

          <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">Scan roots</h3>
          <p class="mt-1 text-xs text-zinc-500">
            Folders DevClean looks in. Cleanup outside these roots is always refused by the safety
            validator — adding a root widens where caches are found, never what may be deleted.
          </p>

          <div class="mt-3 flex flex-wrap gap-2">
            <button
              class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-200 px-3 py-1.5 text-xs font-medium hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
              onclick={() => settingsStore.detect()}
              disabled={settingsStore.detecting}
              data-testid="detect-folders"
            >
              <span aria-hidden="true" class="flex"><Compass size={13} /></span>
              {settingsStore.detecting ? "Looking…" : "Detect folders"}
            </button>
          </div>

          {#if addable.length > 0}
            <p class="mt-3 text-xs font-semibold text-zinc-600 dark:text-zinc-300">
              Suggested folders
            </p>
            <ul class="mt-1.5 space-y-1.5" data-testid="root-candidates">
              {#each addable as c (c.path)}
                <li
                  class="flex items-center justify-between gap-2 rounded-lg bg-zinc-50 px-2.5 py-1.5 dark:bg-zinc-800/50"
                >
                  <span class="min-w-0 flex-1">
                    <span class="block truncate font-mono text-xs" title={c.path}>{c.path}</span>
                    <span class="block text-[11px] text-zinc-400">
                      ~{formatBytes(c.approximate_bytes ?? 0)} in {c.cache_count}
                      {c.cache_count === 1 ? "cache" : "caches"}
                    </span>
                  </span>
                  <button
                    class="shrink-0 rounded border border-zinc-200 px-2 py-0.5 text-[11px] hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-700"
                    onclick={() => addCandidate(c)}
                  >
                    Add
                  </button>
                </li>
              {/each}
            </ul>
          {:else if alreadyAdded > 0}
            <p class="mt-3 text-xs text-zinc-400">
              {alreadyAdded} suggested {alreadyAdded === 1 ? "folder is" : "folders are"} already
              a scan root.
            </p>
          {/if}

          <ul class="mt-3 space-y-1.5">
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
          {#if roots.length === 0}
            <p class="mt-2 text-xs text-amber-700 dark:text-amber-400">
              At least one scan root is required — without one, project caches such as
              node_modules cannot be found.
            </p>
          {/if}
          <div class="mt-2 flex gap-2">
            <input
              type="text"
              class="min-w-0 flex-1 rounded-lg border border-zinc-200 bg-transparent px-2 py-1.5 font-mono text-xs dark:border-zinc-700"
              placeholder="/Users/you/Projects"
              bind:value={newRoot}
              onkeydown={(e) => {
                if (e.key === "Enter") addRoot(newRoot);
              }}
              aria-label="New scan root path"
            />
            <button
              class="shrink-0 rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
              onclick={() => addRoot(newRoot)}
              disabled={!newRoot.trim()}
            >
              <span aria-hidden="true" class="flex"><Plus size={14} /></span>
            </button>
          </div>

          <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">Ignore list</h3>
          <p class="mt-1 text-xs text-zinc-500">
            Projects the scan skips. This is a reporting filter, not a safety control — the safety
            validator still decides what may be deleted.
          </p>
          <ul class="mt-2 space-y-1.5">
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
                if (e.key === "Enter") addExcluded(newExcluded);
              }}
              aria-label="New ignored path"
            />
            <button
              class="shrink-0 rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
              onclick={() => addExcluded(newExcluded)}
              disabled={!newExcluded.trim()}
            >
              <span aria-hidden="true" class="flex"><Plus size={14} /></span>
            </button>
          </div>
        </div>
      {:else if tab === "cleanup"}
        <div id="settings-panel-cleanup" role="tabpanel" aria-label="Cleanup settings">
          <h3 class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">Confirmation</h3>
          <label class="mt-2 flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              class="mt-0.5 h-4 w-4 accent-emerald-600"
              bind:checked={requireConfirmation}
            />
            <span>
              Ask for confirmation before cleanup
              <span class="block text-xs text-zinc-500">
                With this on, the backend refuses any cleanup that was not previewed first — the
                dialog is not just a habit.
              </span>
            </span>
          </label>
          <label class="mt-3 flex items-start gap-2 text-sm">
            <input type="checkbox" class="mt-0.5 h-4 w-4 accent-emerald-600" bind:checked={showPreview} />
            <span>
              Show the item list in the confirmation dialog
              <span class="block text-xs text-zinc-500">
                Off shows only the total. The list is still re-measured either way.
              </span>
            </span>
          </label>

          <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">Retention</h3>
          <p class="mt-1 text-xs text-zinc-500">
            Keep anything younger than this out of cleanup. Items you keep are still listed, with
            their sizes — they are just not selectable.
          </p>
          <div class="mt-2 flex flex-wrap items-center gap-2">
            <label class="flex items-center gap-2 text-sm text-zinc-600 dark:text-zinc-300">
              <input
                type="number"
                min="0"
                max="3650"
                class="field tnum w-24"
                value={retentionDays ?? ""}
                oninput={(e) => {
                  const v = (e.currentTarget as HTMLInputElement).value.trim();
                  retentionDays = v === "" ? null : Math.max(0, Number(v));
                }}
                aria-label="Keep items younger than this many days"
              />
              days
            </label>
            <span class="text-xs text-zinc-500">or</span>
            <!-- Same segmented pattern as the Theme buttons, and the same
                 .control box as the field next to it: one height, one
                 padding, one radius. -->
            <button
              class="control border-zinc-200 hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800 {retentionDays ===
              null
                ? 'border-emerald-600 bg-emerald-50 font-semibold text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200'
                : ''}"
              onclick={() => (retentionDays = null)}
              disabled={retentionDays === null}
              aria-pressed={retentionDays === null}
            >
              No age limit
            </button>
          </div>
          <p class="mt-1.5 text-xs text-zinc-500">
            {#if retentionDays === null}
              Currently: no age policy — everything found is offered.
            {:else}
              Currently: items younger than {retentionDays} {retentionDays === 1 ? "day" : "days"}
              are held back.
            {/if}
            Applies to per-project and AI history items, not to whole-tool caches — a package
            cache has no meaningful age.
          </p>

          <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">Docker</h3>
          <label class="mt-2 block text-sm">
            <span class="sr-only">Docker cleanup scope</span>
            <select
              class="control w-full"
              bind:value={dockerScope}
              data-testid="docker-scope"
              aria-label="Docker cleanup scope"
            >
              {#each Object.entries(DOCKER_SCOPE_LABELS) as [value, label] (value)}
                <option {value}>{label}</option>
              {/each}
            </select>
          </label>
          <p class="mt-1.5 text-xs text-zinc-500">{scopeConsequence}</p>

          <label class="mt-3 flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              class="mt-0.5 h-4 w-4 accent-red-600"
              bind:checked={allowPrivileged}
              data-testid="allow-privileged"
            />
            <span>
              <span class="inline-flex items-center gap-1.5">
                <span aria-hidden="true" class="flex text-amber-600"
                  ><ShieldAlert size={13} /></span
                >
                Privileged operations
              </span>
              <span class="block text-xs text-zinc-500">
                Required for anything past the build cache. With it off, the widest scope that
                actually runs is
                <strong class="font-medium">{DOCKER_SCOPE_LABELS[scopeEffective]}</strong>.
              </span>
            </span>
          </label>
          {#if scopeNeedsPrivileged && !allowPrivileged}
            <p
              class="mt-2 rounded-lg bg-amber-50 px-2.5 py-2 text-xs text-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
              data-testid="scope-locked"
            >
              You selected “{scopeLabel}”, which needs Privileged operations. Until you enable
              it, Docker cleanup stays at build cache.
            </p>
          {/if}

          <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">
            AI history safety
          </h3>
          <label class="mt-2 flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              class="mt-0.5 h-4 w-4 accent-emerald-600"
              bind:checked={sqliteBackup}
              data-testid="sqlite-backup"
            />
            <span>
              Back up the conversation database before deleting from it
              <span class="block text-xs text-zinc-500">
                Off by default: the backup is a full copy, and the opencode database is around
                13 GB. With it off you still see the size and the risk before confirming.
              </span>
            </span>
          </label>
          {#if settingsStore.diskFree}
            <p class="mt-1.5 text-xs text-zinc-500">
              Free space on this volume:
              {#if settingsStore.diskFree.known}
                <strong class="font-medium"
                  >{formatBytes(settingsStore.diskFree.available_bytes)}</strong
                >
                of {formatBytes(settingsStore.diskFree.total_bytes)}
              {:else}
                unknown
              {/if}
            </p>
          {/if}
        </div>
      {:else if tab === "appearance"}
        <div id="settings-panel-appearance" role="tabpanel" aria-label="Appearance settings">
          <h3 class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">Theme</h3>
          <p class="mt-1 text-xs text-zinc-500">
            Currently: {theme === "system" ? "follow the system" : theme === "dark" ? "dark" : "light"}.
          </p>
          <div class="mt-2 flex gap-2" role="radiogroup" aria-label="Theme">
            {#each ["system", "light", "dark"] as const as t (t)}
              <button
                role="radio"
                aria-checked={theme === t}
                class="rounded-lg border px-3 py-1.5 text-sm capitalize {theme === t
                  ? 'border-emerald-600 bg-emerald-50 font-medium text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200'
                  : 'border-zinc-200 hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800'}"
                onclick={() => (theme = t)}
              >
                {t}
              </button>
            {/each}
          </div>

          <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">Rows</h3>
          <p class="mt-1 text-xs text-zinc-500">
            The <strong class="font-medium">Paths</strong> toggle in the list toolbar switches the
            filesystem path under each item on or off. It is remembered for this session, so there
            is nothing to save here.
          </p>
        </div>
      {:else}
        <div id="settings-panel-advanced" role="tabpanel" aria-label="Advanced settings">
          <h3 class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">Scheduling</h3>
          <label class="mt-2 flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              class="mt-0.5 h-4 w-4 accent-emerald-600"
              bind:checked={scanAutomatically}
            />
            <span>
              Scan automatically at startup
              <span class="block text-xs text-zinc-500">
                A scan runs on every launch when enabled. Cleanup is never automatic — it always
                goes through the confirmation dialog.
              </span>
            </span>
          </label>

          <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">What is never
            touched</h3>
          <p class="mt-1 inline-flex items-start gap-2 text-xs text-zinc-500">
            <span aria-hidden="true" class="mt-0.5 flex"><Info size={13} /></span>
            <span>
              Documents, Desktop, Downloads, Pictures, Videos, .ssh, .gnupg and .config are
              hard-protected: no scan root, provider or setting can make them cleanable. AI
              settings and credentials are never pruned either, whatever the pattern matches.
            </span>
          </p>

          <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">Reset</h3>
          <p class="mt-1 text-xs text-zinc-500">
            Returns every unsaved change on these tabs to the shipped defaults. Saving then
            re-enables confirmation and turns privileged operations back off.
          </p>
          <button
            class="mt-2 inline-flex items-center gap-1.5 rounded-lg border border-red-200 px-3 py-1.5 text-sm text-red-700 hover:bg-red-50 dark:border-red-900 dark:text-red-300 dark:hover:bg-red-950"
            onclick={resetAll}
            data-testid="reset-settings"
          >
            <span aria-hidden="true" class="flex"><RotateCcw size={13} /></span>
            Reset to defaults
          </button>
        </div>
      {/if}

      {#if error}
        <p
          class="mt-4 rounded-lg bg-red-50 p-2 text-sm text-red-700 dark:bg-red-950 dark:text-red-300"
          role="alert"
        >
          {error}
        </p>
      {/if}
    </div>

    <!-- Sticky footer -->
    <footer
      class="flex shrink-0 items-center justify-between gap-3 border-t border-zinc-200 px-5 py-3 dark:border-zinc-800"
    >
      <p class="text-xs text-zinc-400">
        Stored on disk, reloaded at startup · {activeCount} of {providers.length} providers active
      </p>
      <div class="flex items-center gap-2">
        <button
          class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 disabled:opacity-50 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={onClose}
          disabled={saving}
        >
          Cancel
        </button>
        <button
          class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
          onclick={save}
          disabled={saving || roots.length === 0 || !dirty}
          title={roots.length === 0 ? "At least one scan root is required" : ""}
          data-testid="settings-save"
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </footer>
  </div>
</div>