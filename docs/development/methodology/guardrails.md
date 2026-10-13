# Guardrails (lean edition)

> Constraints that keep DevClean out of unacceptable states.
> - A **test** answers: *did we observe the expected behavior?*
> - A **guardrail** answers: *what behavior is never allowed?*
>
> Every entry names its **enforcement mechanism** — without enforcement, a
> guardrail is just a suggestion. System model: [system-model.md](./system-model.md).

## Reading the enforcement column

- **code** — enforced by program logic · **config** — enforced by shipped defaults ·
- **test** — covered by an automated test · **CI** — enforced by a CI job ·
- **process** — enforced by the DoD/review workflow · `(gap)` — missing, tracked honestly

## 1. Safety (G-SAFE) — the life of this product

```text
G-SAFE-01  The frontend must not supply paths for deletion; resource_id only.
           Enforcement: code — commands resolve id→path server-side via state;
           test — resolve_rejects_unknown_id ✅
G-SAFE-02  No shell commands from user input; only fixed provider (program, args) pairs.
           Enforcement: code — CleanupAction::RunCommand enum; review ✅
G-SAFE-03  Significant destructive operations are not defaults (volumes, venvs, images).
           Enforcement: config — Docker only prunes build cache; allow_privileged OFF ✅
G-SAFE-04  Protected paths are untouchable from any direction (==, inside, ancestor).
           Enforcement: code+test — SafetyValidator, 7 tests ✅
```

## 2. Scan (G-SCAN)

```text
G-SCAN-01  Scan concurrency is bounded (max 4 providers in parallel).
           Enforcement: code — tokio Semaphore in ScanManager ✅
G-SCAN-02  Scan & cleanup are cancellable, settling in <1s.
            Enforcement: code — cooperative AtomicBool checks per walk entry ✅ ·
            test — cancel_settles_in_under_one_second ✅
G-SCAN-03  Symlinks are never followed during traversal/measurement.
           Enforcement: code — follow_links(false) ✅
```

## 3. Contract (G-API)

```text
G-API-01  Command/event/response changes require a tauri-contract.md update + test.
           Enforcement: process — DoD step 2, review ✅
G-API-02  Cleanup status/error codes are deterministic per resource (I8).
           Enforcement: code — CleanupResult {status, freed, error} ✅
```

## 4. Reliability (G-REL)

```text
G-REL-01  No external operation may block forever.
            Enforcement: code — spawn_blocking + stdin-less commands ✅ ·
            explicit timeouts: PROVIDER_SCAN_TIMEOUT (60s) + CLEANUP_COMMAND_TIMEOUT
            (120s, child killed) ✅ · test — provider_timeout + command_timeout ✅
G-REL-02  Partial failure is transparent to the user.
           Enforcement: code — engine computes freed = before−after per resource ✅
```

## 5. UI (G-UI)

```text
G-UI-01  Significant cleanup requires a confirmation dialog + re-measured preview.
           Enforcement: code — require_confirmation/show_preview default ON;
           frontend ConfirmDialog ✅
```

## 6. CI & AI (G-CI / G-AI)

```text
G-CI-01  Merging requires: cargo test + clippy 0 warnings + clean fmt + svelte-check 0 errors.
           Enforcement: CI — .github/workflows/ci.yml ✅
G-AI-01  AI Level A is for internal & reversible changes only. Contract/security/destructive/
          dependency changes = Level B, requiring explicit human approval. See AGENTS.md.
           Enforcement: process ✅
```

## Open gaps

(none — G-SCAN-02 and G-REL-01 gaps closed in M3.)
