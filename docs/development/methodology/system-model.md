# System Model (lean edition)

> The canonical DevClean model: what system we are building, where its
> boundaries are, and which properties must always hold. Guardrails live in
> [guardrails.md](./guardrails.md).

## 1. System boundary

```text
Svelte UI (untrusted)
   │  invoke(commands) / listen(events)
   ▼
src-tauri: commands + state (thin wiring, no filesystem logic)
   │
   ├─ ScanManager ──→ providers (node / rust / go / docker)
   ├─ CleanupEngine ─→ providers.cleanup_action()
   └─ SafetyValidator (gatekeeper, from core)
          │
          ▼
     OS filesystem / `go` / `docker` CLI
```

External dependencies are part of the model: the `go` and `docker` CLIs may
be absent (providers return empty — absence is normal, not an error).

## 2. Desired outcomes

Every non-trivial change must trace to at least one outcome:

1. **Accurate** — everything reported is genuinely reclaimable, no false positives.
2. **Safe** — the Clean button never deletes what it shouldn't.
3. **Fast** — startup <2s, progress updates every 100–500ms, cancel <1s.
4. **Transparent** — partial failure is reported per resource, never as a generic "failed".
5. **Recoverable** — everything deleted is regenerable (`npm install`, `cargo build`, `go build`, Docker layer rebuild).

## 3. Architecture principles

- **P1 — Providers, not `if/else`.** A new resource kind = one `Provider` trait impl + one entry in `all_providers()`. The backend must not branch per ecosystem outside the trait.
- **P2 — Filesystem-blind frontend.** The UI never holds paths as capabilities; it holds opaque `resource_id`s and renders them as text.
- **P3 — One door for deletion.** All destruction flows through `CleanupEngine`: `RemoveDir` must pass the validator, `RunCommand` must be a fixed pair.
- **P4 — Measure twice.** `preview` re-measures at confirmation time (scan numbers can go stale); `execute` reports freed = before − after.
- **P5 — Official tools first.** Go via `go clean`, Docker via `docker builder prune`. Never poke at cache internals.

## 4. Core invariants

```text
I1. The frontend never supplies a raw path for deletion; it supplies a resource_id.
I2. Directory deletion only happens for SafetyValidator-approved paths.
I3. Docker is never cleaned by reading/deleting its internal storage.
I4. Go is never cleaned by hand-deleting cache contents.
I5. Scan and cleanup are cancellable and settle in <1s.
I6. Cleanup is always preceded by a re-measured preview (unless settings disable
    preview — default ON).
I7. Protected paths (/, ~/Documents, ~/.ssh, …) are untouchable even if a provider claims them.
I8. Every cleanup result is per resource: Success / Partial / Failed / Cancelled + numbers + reason.
```

An invariant violation is release-blocking.
