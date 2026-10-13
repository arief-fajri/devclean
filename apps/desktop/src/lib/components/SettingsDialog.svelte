<script lang="ts">
  import type { Settings } from "$lib/tauri";

  interface Props {
    settings: Settings;
    saving: boolean;
    error: string | null;
    onSave: (next: Settings) => void;
    onClose: () => void;
  }

  let { settings, saving, error, onSave, onClose }: Props = $props();

  // Draft lokal, di-seed dari props sekali saat dialog mount
  // (dialog selalu fresh-mount via {#if}, jadi seed-once sudah benar).
  let roots = $state<string[]>([]);
  let newRoot = $state("");
  let requireConfirmation = $state(true);
  let showPreview = $state(true);
  let allowPrivileged = $state(false);
  let seeded = $state(false);
  $effect.pre(() => {
    if (!seeded) {
      roots = [...settings.scan_roots];
      requireConfirmation = settings.require_confirmation;
      showPreview = settings.show_preview;
      allowPrivileged = settings.allow_privileged_operations;
      seeded = true;
    }
  });

  function addRoot() {
    const p = newRoot.trim();
    if (p && !roots.includes(p)) roots = [...roots, p];
    newRoot = "";
  }

  function removeRoot(p: string) {
    roots = roots.filter((r) => r !== p);
  }

  function save() {
    onSave({
      ...settings,
      scan_roots: roots,
      require_confirmation: requireConfirmation,
      show_preview: showPreview,
      allow_privileged_operations: allowPrivileged,
    });
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
  role="dialog"
  aria-modal="true"
  aria-label="Settings"
>
  <div class="w-full max-w-md rounded-xl bg-white p-6 shadow-xl dark:bg-zinc-900">
    <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-100">Settings</h2>

    <h3 class="mt-4 text-sm font-semibold text-zinc-700 dark:text-zinc-300">Scan roots</h3>
    <p class="mt-1 text-xs text-zinc-500">
      Sumber scan. Cleanup di luar roots selalu ditolak validator (I2).
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
            aria-label="Hapus {r}"
          >
            Hapus
          </button>
        </li>
      {/each}
    </ul>
    <div class="mt-2 flex gap-2">
      <input
        type="text"
        class="min-w-0 flex-1 rounded-lg border border-zinc-200 bg-transparent px-2 py-1.5 font-mono text-xs dark:border-zinc-700"
        placeholder="/Users/kamu/Projects"
        bind:value={newRoot}
        onkeydown={(e) => {
          if (e.key === "Enter") addRoot();
        }}
        aria-label="Path scan root baru"
      />
      <button
        class="shrink-0 rounded-lg border border-zinc-200 px-3 py-1.5 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-800"
        onclick={addRoot}
      >
        + Tambah
      </button>
    </div>

    <div class="mt-4 space-y-2 text-sm">
      <label class="flex items-center gap-2">
        <input type="checkbox" class="h-4 w-4 accent-emerald-600" bind:checked={requireConfirmation} />
        Minta konfirmasi sebelum cleanup (G-UI-01)
      </label>
      <label class="flex items-center gap-2">
        <input type="checkbox" class="h-4 w-4 accent-emerald-600" bind:checked={showPreview} />
        Tampilkan daftar item di dialog (I6)
      </label>
      <label class="flex items-start gap-2">
        <input type="checkbox" class="mt-0.5 h-4 w-4 accent-red-600" bind:checked={allowPrivileged} />
        <span>
          Operasi privileged
          <span class="block text-xs text-amber-700 dark:text-amber-300">
            Default OFF (G-SAFE-03). Jangan aktifkan kecuali kamu paham risikonya.
          </span>
        </span>
      </label>
    </div>

    {#if error}
      <p class="mt-3 rounded-lg bg-red-50 p-2 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
        {error}
      </p>
    {/if}
    <p class="mt-3 text-xs text-zinc-400">Tersimpan untuk sesi ini (persistensi file = M3).</p>

    <div class="mt-4 flex justify-end gap-2">
      <button
        class="rounded-lg px-4 py-2 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
        onclick={onClose}
        disabled={saving}
      >
        Batal
      </button>
      <button
        class="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-700 disabled:opacity-50"
        onclick={save}
        disabled={saving || roots.length === 0}
        title={roots.length === 0 ? "Minimal satu scan root" : ""}
      >
        {saving ? "Menyimpan…" : "Simpan"}
      </button>
    </div>
  </div>
</div>
