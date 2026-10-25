<script lang="ts">
  /**
   * Settings page. Was a modal with a tab strip; now a page with a vertical
   * section nav. The draft state below is deliberately local: leaving the
   * page discards unsaved edits, and the sticky bar says so out loud rather
   * than letting a silent save/lose surprise the user.
   */
  import { formatBytes, volumeLabel } from "$lib/format";
  import { SETTINGS_TABS, settingsTab } from "$lib/nav";
  import { providerMeta } from "$lib/providerMeta";
  import { navStore } from "$lib/stores/nav.svelte";
  import { scanStore } from "$lib/stores/scan.svelte";
  import { settingsStore } from "$lib/stores/settings.svelte";
  import { updateStore } from "$lib/stores/update.svelte";
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
  import PageHeader from "$lib/components/PageHeader.svelte";
  import { Compass, Info, Plus, RotateCcw, ShieldAlert } from "lucide-svelte";

  interface Props {
    /** Commit handler: persists, re-applies the theme and rescans when the
     *  roots or the provider set moved. */
    onSave: (next: Settings) => void;
  }

  let { onSave }: Props = $props();

  const settings = $derived(settingsStore.settings);
  /** From `get_providers` — the backend registry is the source of truth. */
  const providers = $derived(settingsStore.providers);
  const saving = $derived(settingsStore.saving);
  const error = $derived(settingsStore.error);
  /** Live updater state, mirrored from the backend (contract v0.12.0). */
  const update = $derived(updateStore.status);

  /** Last scan results per provider id, for a live size hint. */
  const providerStats = $derived.by(() => {
    const out: Record<string, { count: number; bytes: number }> = {};
    for (const p of scanStore.summary?.providers ?? []) {
      out[p.provider] = { count: p.resource_count, bytes: p.reclaimable_bytes };
    }
    return out;
  });

  // The draft is seeded from the persisted settings; until they arrive there
  // is nothing to edit.
  const loaded = $derived(!!settings);

  const tab = $derived(navStore.settingsTab);

  // Draft state, seeded once from props. It survives section switches and
  // the whole navigation, so moving around never discards half-finished edits.
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
  let aiCleanup = $state(false);
  let scanAutomatically = $state(false);
  let checkUpdatesAutomatically = $state(false);
  let seeded = $state(false);

  function seed() {
    if (!settings) return;
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
    aiCleanup = settings.ai_cleanup_enabled ?? false;
    scanAutomatically = settings.scan_automatically ?? false;
    checkUpdatesAutomatically = settings.check_updates_automatically ?? false;
    void settingsStore.refreshCandidates();
  }

  $effect.pre(() => {
    if (!seeded) {
      seed();
      seeded = true;
    }
  });

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
  /** True when the draft differs from what is on disk. */
  const dirty = $derived.by(() => {
    if (!settings) return false;
    return (
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
      aiCleanup !== (settings.ai_cleanup_enabled ?? false) ||
      scanAutomatically !== (settings.scan_automatically ?? false) ||
      checkUpdatesAutomatically !== (settings.check_updates_automatically ?? false)
    );
  });

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
    aiCleanup = false;
    scanAutomatically = false;
    checkUpdatesAutomatically = false;
    newRoot = "";
    newExcluded = "";
    void settingsStore.refreshCandidates();
  }

  let tablistEl: HTMLDivElement | undefined = $state();

  /** Arrow keys move between tabs; Home/End jump to the ends. */
  function onTabKey(e: KeyboardEvent) {
    const i = SETTINGS_TABS.findIndex((t) => t.id === navStore.settingsTab);
    let next: number | null = null;
    if (e.key === "ArrowRight") next = (i + 1) % SETTINGS_TABS.length;
    else if (e.key === "ArrowLeft") next = (i - 1 + SETTINGS_TABS.length) % SETTINGS_TABS.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = SETTINGS_TABS.length - 1;
    if (next === null) return;
    e.preventDefault();
    navStore.settingsTab = SETTINGS_TABS[next].id;
    tablistEl
      ?.querySelector<HTMLElement>(`#settings-tab-${SETTINGS_TABS[next].id}`)
      ?.focus();
  }

  /** Throw away every unsaved edit and show what is actually on disk. */
  function discard() {
    seed();
  }

  /** " (just now)" / " (12m ago)" for a completed check, or "". */
  function checkedSuffix(unix: number | null | undefined): string {
    if (!unix) return "";
    const mins = Math.floor((Date.now() / 1000 - unix) / 60);
    if (mins < 1) return " (just now)";
    if (mins < 60) return ` (${mins}m ago)`;
    return "";
  }

  function save() {
    if (!settings) return;
    onSave({
      ...settings,
      scan_roots: roots,
      excluded_paths: excluded,
      disabled_providers: disabledProviders,
      require_confirmation: requireConfirmation,
      show_preview: showPreview,
      allow_privileged_operations: allowPrivileged,
      scan_automatically: scanAutomatically,
      check_updates_automatically: checkUpdatesAutomatically,
      theme,
      retention_days: retentionDays,
      docker_prune_scope: dockerScope,
      sqlite_backup: sqliteBackup,
      ai_cleanup_enabled: aiCleanup,
    });
  }
</script>

<!-- pb-28 clears the fixed save bar at the bottom of the column. -->
<div class="pb-28">
  <PageHeader title="Settings" hint={settingsTab(tab).hint} />

  {#if !loaded}
    <p class="text-sm text-zinc-500" role="status">Loading settings…</p>
  {:else}
    <!-- Sections as a tab strip: the page is one column now, so a left rail
         would only eat width the provider list and forms need. -->
    <div class="border-b border-zinc-200 dark:border-zinc-800">
      <div
        bind:this={tablistEl}
        class="-mb-px flex gap-1 overflow-x-auto"
        aria-label="Settings sections"
        role="presentation"
        onkeydown={onTabKey}
      >
        {#each SETTINGS_TABS as t (t.id)}
          {@const active = tab === t.id}
          <button
            role="tab"
            id="settings-tab-{t.id}"
            data-testid="settings-tab-{t.id}"
            aria-selected={active}
            aria-controls="settings-panel-{t.id}"
            tabindex={active ? 0 : -1}
            class="border-b-2 px-3 py-2.5 text-sm font-medium whitespace-nowrap transition-colors {active
              ? 'border-emerald-600 text-zinc-900 dark:text-zinc-50'
              : 'border-transparent text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200'}"
            onclick={() => (navStore.settingsTab = t.id)}
          >
            {t.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="mt-5">
      {#if navStore.settingsTab === "providers"}
      <div id="settings-panel-providers" role="tabpanel" aria-labelledby="settings-tab-providers">
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
                <label class="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500">
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

      </div>
      {:else if navStore.settingsTab === "cleanup"}
      <div id="settings-panel-cleanup" role="tabpanel" aria-labelledby="settings-tab-cleanup">
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

        <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">
          When something is cleaned
        </h3>
        <p class="mt-1 text-xs text-zinc-500">
          Every cleanup is permanent since 1.0.0 — deleted files reclaim space
          immediately and cannot be undone. Review the re-measured preview and
          confirm before cleaning.
        </p>
        <p
          class="mt-2 rounded-lg bg-amber-50 px-2.5 py-2 text-xs text-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
          data-testid="permanent-warning"
        >
          Permanent deletion has no undo.
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
          AI history
        </h3>
        <label class="mt-2 flex items-start gap-2 text-sm">
          <input
            type="checkbox"
            class="mt-0.5 h-4 w-4 accent-emerald-600"
            bind:checked={aiCleanup}
            data-testid="ai-cleanup-enabled"
          />
          <span>
            Allow cleaning AI conversation history
            <span class="block text-xs text-zinc-500">
              Off by default. Session transcripts, project memory and agent state cannot be
              regenerated, so they are reported with their sizes but stay unselectable until you
              turn this on. Nothing is deleted by enabling it — it only makes those rows
              selectable.
            </span>
          </span>
        </label>
        {#if !aiCleanup}
          <p
            class="mt-2 rounded-lg bg-zinc-100 px-2.5 py-2 text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
            data-testid="ai-report-only"
          >
            AI history is currently <strong class="font-medium">report only</strong>.
          </p>
        {/if}
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
            {volumeLabel(settingsStore.diskFree)}:
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
      {:else if navStore.settingsTab === "appearance"}
      <div id="settings-panel-appearance" role="tabpanel" aria-labelledby="settings-tab-appearance">
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
      {:else if navStore.settingsTab === "updates"}
      <div id="settings-panel-updates" role="tabpanel" aria-labelledby="settings-tab-updates">
        <h3 class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">Version</h3>
        <p class="mt-1 text-xs text-zinc-500">
          DevClean checks GitHub Releases for a newer version. Nothing is downloaded until you ask
          for it, and the download is signature-verified before it is installed.
        </p>
        <div
          class="mt-3 flex items-center justify-between gap-3 rounded-lg bg-zinc-100 px-3 py-2 dark:bg-zinc-800"
        >
          <span class="text-sm text-zinc-700 dark:text-zinc-300">
            DevClean <strong class="font-medium">{update?.current_version ?? "…"}</strong>
          </span>
          <button
            class="rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 disabled:opacity-50 dark:border-zinc-700 dark:hover:bg-zinc-700"
            onclick={() => updateStore.check()}
            disabled={updateStore.busy ||
              update?.phase === "checking" ||
              update?.phase === "downloading" ||
              update?.phase === "installing"}
            data-testid="check-updates"
          >
            {update?.phase === "checking" ? "Checking…" : "Check for updates"}
          </button>
        </div>

        {#if update?.phase === "up_to_date"}
          <p class="mt-2 text-xs text-zinc-500" data-testid="update-up-to-date">
            You are on the latest version{checkedSuffix(update.checked_at_unix)}.
          </p>
        {:else if update?.phase === "available"}
          <div
            class="mt-3 rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 dark:border-emerald-900 dark:bg-emerald-950/40"
            data-testid="update-available"
          >
            <p class="text-sm font-medium text-emerald-800 dark:text-emerald-200">
              Version {update.available_version} is available
            </p>
            {#if update.notes}
              <p
                class="mt-1 max-h-40 overflow-auto text-xs whitespace-pre-wrap text-emerald-900/80 dark:text-emerald-200/80"
              >
                {update.notes}
              </p>
            {/if}
            <button
              class="mt-2 rounded-lg bg-emerald-600 px-3 py-1.5 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
              onclick={() => updateStore.install()}
              disabled={updateStore.busy}
              data-testid="install-update"
            >
              Download &amp; install
            </button>
          </div>
        {:else if update?.phase === "downloading"}
          <div class="mt-3" data-testid="update-downloading">
            <p class="text-xs text-zinc-500">
              Downloading {update.available_version}…
              {#if update.progress_percent != null}{update.progress_percent}%{/if}
            </p>
            <div class="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-700">
              <div
                class="h-full bg-emerald-600 transition-[width]"
                style="width: {update.progress_percent ?? 15}%"
              ></div>
            </div>
          </div>
        {:else if update?.phase === "installing"}
          <p class="mt-2 text-xs text-zinc-500" data-testid="update-installing">Installing…</p>
        {:else if update?.phase === "ready_to_restart"}
          <div
            class="mt-3 rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 dark:border-emerald-900 dark:bg-emerald-950/40"
            data-testid="update-ready"
          >
            <p class="text-sm text-emerald-800 dark:text-emerald-200">
              Version {update.available_version} is installed.
            </p>
            <button
              class="mt-2 rounded-lg bg-emerald-600 px-3 py-1.5 text-sm font-semibold text-white hover:bg-emerald-700"
              onclick={() => updateStore.restart()}
              data-testid="restart-app"
            >
              Restart to apply
            </button>
          </div>
        {:else if update?.phase === "error"}
          <p
            class="mt-2 rounded-lg bg-red-50 px-2.5 py-2 text-xs text-red-700 dark:bg-red-950 dark:text-red-300"
            role="alert"
            data-testid="update-error"
          >
            {update.error ?? "The update check failed."}
          </p>
        {:else if update?.phase === "idle"}
          <p class="mt-2 text-xs text-zinc-500">Not checked yet this session.</p>
        {/if}

        {#if updateStore.error}
          <p
            class="mt-2 rounded-lg bg-red-50 px-2.5 py-2 text-xs text-red-700 dark:bg-red-950 dark:text-red-300"
            role="alert"
          >
            {updateStore.error}
          </p>
        {/if}

        <h3 class="mt-6 text-sm font-semibold text-zinc-900 dark:text-zinc-100">
          Automatic checks
        </h3>
        <label class="mt-2 flex items-start gap-2 text-sm">
          <input
            type="checkbox"
            class="mt-0.5 h-4 w-4 accent-emerald-600"
            bind:checked={checkUpdatesAutomatically}
            data-testid="check-updates-automatically"
          />
          <span>
            Check for updates at startup
            <span class="block text-xs text-zinc-500">
              Off by default. When it is off, DevClean makes no outbound request until you press
              Check for updates.
            </span>
          </span>
        </label>
      </div>
      {:else}
      <div id="settings-panel-advanced" role="tabpanel" aria-labelledby="settings-tab-advanced">
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

    <!-- Save bar: fixed, not sticky. `sticky` only pins while the content is
         taller than the viewport, so on a short section it drifted up into the
         middle of the window. -->
    <div
      class="fixed right-0 bottom-0 left-sidebar z-20 flex flex-wrap items-center justify-between gap-3 border-t border-zinc-200 bg-zinc-50 px-6 py-3 dark:border-zinc-800 dark:bg-zinc-950"
    >
      <p class="text-xs {dirty ? 'text-amber-700 dark:text-amber-300' : 'text-zinc-400'}">
        {#if dirty}Unsaved changes{:else}Saved on disk, reloaded at startup{/if} · {activeCount} of
        {providers.length} providers active
      </p>
      <div class="flex items-center gap-2">
        <button
          class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 disabled:opacity-50 dark:text-zinc-300 dark:hover:bg-zinc-800"
          onclick={discard}
          disabled={saving || !dirty}
        >
          Discard changes
        </button>
        <button
          class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
          onclick={save}
          disabled={saving || roots.length === 0 || !dirty}
          title={roots.length === 0 ? "At least one scan root is required" : ""}
          data-testid="settings-save"
        >
          {saving ? "Saving…" : "Save changes"}
        </button>
      </div>
    </div>
  {/if}
</div>
