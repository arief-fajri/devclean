# DevClean Architecture

## Components

```text
┌─────────────────────────────────────────────────┐
│                    DevClean                     │
├─────────────────────────────────────────────────┤
│ Svelte 5 (runes) + Tailwind                     │
│   App.svelte shell: Sidebar + lib/routes/*      │
│   stores: nav, scan, cleanup, settings          │
│   bridge: lib/tauri.ts (invoke + listen, typed) │
│        │ Tauri commands / events                │
│        ▼                                        │
│ src-tauri (thin): commands · events · state     │
│        · tray (menu bar; read-only, no cleanup)  │
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
│        ▼                                        │
│         OS filesystem / go / docker CLI / pgrep │
└─────────────────────────────────────────────────┘
```

## Critical data flows

Scan:

```text
UI scan_all() → ScanManager buckets active providers (semaphore, max 4 tasks)
  → OwnScanner providers: spawn_blocking scan per provider
  → TreeRule providers: ONE shared walk_tree_rules for all tree rules
  → registry assigns ids (nm:0001…) → state stores id→path
  → emit scan_progress/* (one tick per provider) → UI renders
```

Cleanup (the only door):

```text
UI selects ids → preview_cleanup() [re-measure + warnings]
  → CleanupFlowDialog (the one modal, shell-mounted) → cleanup(ids)
  → resolve id→path server-side → SafetyValidator
  → RemoveDir: remove_dir_all | RunCommand: fixed (program, args)
  → freed = before − after → emit cleanup_completed → UI result
```

## Key decisions

1. **Sync traits, async manager.** A provider's scan is synchronous (walkdir is blocking anyway); concurrency belongs to `ScanManager` via `spawn_blocking`. `Provider` carries identity/cleanup/policy only; scanning lives in `OwnScanner` (a provider walks its own targets) or `TreeRule` (a provider contributes a rule to the shared project-tree walk). `all_providers()` hands out `Scannable`, and every non-scanning consumer reaches the plain `Provider` view. Both traits stay object-safe.
2. **Opaque ids from the registry.** Providers return resources with an empty `id`; `ProviderRegistry` fills in `nm:0001` etc. after the scan. The shared walk keeps its rows grouped per rule so each provider's ids stay sequential.
3. **Docker is virtual.** Its path is a `__docker__/…` sentinel — no path validation applies because no path is ever deleted; only the allowlisted `docker builder prune -af` runs.
4. **State lives in the backend.** `AppState.resources` is the single id→path map. Restarting the app drops the map, so stale ids can never be reused (safe by design).
