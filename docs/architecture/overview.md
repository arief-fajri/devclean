# DevClean Architecture

## Components

```text
┌─────────────────────────────────────────────────┐
│                    DevClean                     │
├─────────────────────────────────────────────────┤
│ Svelte 5 (runes) + Tailwind                     │
│   stores: scan, cleanup, settings               │
│   bridge: lib/tauri.ts (invoke + listen, typed) │
│        │ Tauri commands / events                │
│        ▼                                        │
│ src-tauri (thin): commands · events · state     │
│        │                                        │
│ ┌──────┴──────┐ ┌──────────┐ ┌───────────────┐  │
│ │ ScanManager │ │ Providers│ │ CleanupEngine │  │
│ │ (tokio,     │ │ node/go/ │ │ preview +     │  │
│ │ semaphore 4)│ │ docker/  │ │ executor      │  │
│ └──────┬──────┘ │ rust     │ └───────┬───────┘  │
│        │       └────┬─────┘         │          │
│        └────────────┼───────────────┘          │
│                     ▼                          │
│              SafetyValidator (core)            │
│                     │                          │
│                     ▼                          │
│         OS filesystem / go / docker CLI        │
└─────────────────────────────────────────────────┘
```

## Critical data flows

Scan:

```text
UI scan_all() → ScanManager fans out to 4 providers (semaphore)
  → spawn_blocking walk per provider → registry assigns ids (nm:0001…)
  → state stores id→path → emit scan_progress/* → UI renders
```

Cleanup (the only door):

```text
UI selects ids → preview_cleanup() [re-measure + warnings]
  → ConfirmDialog → cleanup(ids)
  → resolve id→path server-side → SafetyValidator
  → RemoveDir: remove_dir_all | RunCommand: fixed (program, args)
  → freed = before − after → emit cleanup_completed → UI result
```

## Key decisions

1. **Sync trait, async manager.** `Provider::scan` is synchronous (walkdir is
   blocking anyway); concurrency belongs to `ScanManager` via
   `spawn_blocking`. The trait stays object-safe (`Arc<dyn Provider>`).
2. **Opaque ids from the registry.** Providers return resources with an empty
   `id`; `ProviderRegistry` fills in `nm:0001` etc. after the scan.
3. **Docker is virtual.** Its path is a `__docker__/…` sentinel — no path
   validation applies because no path is ever deleted; only the allowlisted
   `docker builder prune -af` runs.
4. **State lives in the backend.** `AppState.resources` is the single id→path
   map. Restarting the app drops the map, so stale ids can never be reused
   (safe by design).
