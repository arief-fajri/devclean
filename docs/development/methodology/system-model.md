# System Model

> An engineering note: the canonical DevClean model — what system we are building, where its boundaries are, and which properties must always hold. It describes how the project is built rather than how to use it. Guardrails live in [guardrails.md](./guardrails.md).

## 1. System boundary

```text
Svelte UI (untrusted)
   │  invoke(commands) / listen(events)
   ▼
src-tauri: commands + state (thin wiring, no filesystem logic)
   │
   ├─ ScanManager ──→ providers (21: node_modules, rust target, Go, Docker, …)
   ├─ CleanupEngine ─→ providers.cleanup_action()
   └─ SafetyValidator (gatekeeper, from core)
          │
          ▼
     OS filesystem / `go` / `docker` CLI
```

External dependencies are part of the model: the `go` and `docker` CLIs may be absent (providers return empty — absence is normal, not an error), and so may `pgrep` (the liveness probe behind `browser_temp`; an unanswerable probe floors that provider's safety rather than blocking or clearing the gate — G-SAFE-07).

## 2. Desired outcomes

Every non-trivial change must trace to at least one outcome:

1. **Accurate** — everything reported is genuinely reclaimable, no false positives.
2. **Safe** — the Clean button never deletes what it shouldn't.
3. **Fast** — startup <2s, progress updates every 100–500ms, cancel <1s.
4. **Transparent** — partial failure is reported per resource, never as a generic "failed".
5. **Regenerable** — almost everything deleted is regenerable via the toolchain (`npm install`, `cargo build`, `go build`, Docker layer rebuild). The exceptions are the items classified **Dangerous** (Xcode Archives with shipped dSYMs, opencode conversation history, Docker local volumes): they are not regenerable, and the confirmation dialog demands a separate, consequence-spelling acknowledgement for them. Since 1.0.0 there is no Trash undo; the safety net is the re-measured preview plus confirmation.

## 3. Architecture principles

- **P1 — Providers, not `if/else`.** A new resource kind = one `Provider` impl plus its scan shape (an `OwnScanner` for a provider that walks its own targets, or a `TreeRule` for one that contributes a rule to the shared project-tree walk), and one entry in `all_providers()`. The backend must not branch per ecosystem outside the trait.
- **P2 — Filesystem-blind frontend.** The UI never holds paths as capabilities; it holds opaque `resource_id`s and renders them as text.
- **P3 — One door for deletion.** All destruction flows through `CleanupEngine`: `RemoveDir` must pass the validator, `RunCommand` must be a fixed pair.
- **P4 — Measure twice.** `preview` re-measures at confirmation time (scan numbers can go stale); `execute` reports freed = before − after.
- **P5 — Official tools first.** Go via `go clean`, Docker via `docker builder prune`. Never poke at cache internals.

## 4. Core invariants

```text
I1.  The frontend never supplies a raw path for deletion; it supplies a resource_id.
I2.  Directory deletion only happens for SafetyValidator-approved paths.
I3.  Docker is never cleaned by reading/deleting its internal storage.
I4.  Go is never cleaned by hand-deleting cache contents.
I5.  Scan and cleanup are cancellable and settle in <1s.
I6.  Cleanup is always preceded by a re-measured preview (unless settings disable
     preview — default ON).
I7.  Protected paths (/, ~/Documents, ~/.ssh, …) are untouchable even if a provider claims them.
I8.  Every cleanup result is per resource: Success / Partial / Failed / Cancelled + numbers + reason.
I9.  Age-based pruning applies ONLY to resources with a meaningful per-item age.
     A whole-tool cache (npm, cargo, go, Docker) has no age and is never held back.
I10. AI configuration and credentials are never prune targets, at any depth, whatever
    the pattern matches.
I11. Retention, the privileged opt-in, the Docker scope and the
     running-browser hold are enforced by the BACKEND. Disabling a
     checkbox in the UI is a convenience; sending the id anyway is refused.
I12. freed_bytes is MEASURED (before − after), never asserted. When the after-state
     cannot be measured, the result reports 0 with a reason.
```

An invariant violation is release-blocking.

### Why I11 and I12 exist

Both were written down after finding that the guardrails claimed enforcement the code did not have:

- **I11** — `require_confirmation` and `allow_privileged_operations` were stored, rendered in the UI, and never read by any backend logic. `cleanup` logged `tracing::debug!("cleanup requires UI confirmation")` and proceeded. Any caller that skipped the dialog deleted immediately.
- **I12** — `freed_bytes` for every `RunCommand` resource equalled `requested_bytes`, because the engine measured the "after" state on the resource's sentinel path (never a directory → always 0). The figure recorded in `history.jsonl` as "freed" was a *scan-time* number replayed as a result, which is why a prune that reclaimed a few gigabytes could report ten.
