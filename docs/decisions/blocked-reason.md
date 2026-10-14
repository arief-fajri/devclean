# ADR: one `blocked_reason`, stamped at scan time

**Status:** accepted (v0.8.0)
**Fixes:** `Settings.retention_days` had no effect on any cleanup

## Context

`Resource.retention_eligible: bool` was introduced with the retention policy
in v0.5.0. It was documented in the contract, surfaced in the UI as a disabled
checkbox, and summarised by the scanner into `retention_blocked_bytes`.

It was also **never written to `false` anywhere in production code**. Verified
by `git log -S "retention_eligible = false"` — the only hits are in test
fixtures. Consequences:

- `ScanSummary.retention_blocked_bytes` summed a filter that could never
  match, so it was permanently `0`.
- The engine's gate (`if !r.retention_eligible { refuse }`) never fired, so a
  resource younger than the retention window was cleaned exactly as if no
  policy existed.
- The UI disabled nothing.

A user could set "keep anything younger than 30 days", watch the setting
persist, and still have a 3-day-old `node_modules` deleted. This is the fourth
instance of the same failure class in this project: **a documented control with
no enforcement mechanism**.

A boolean was also the wrong shape. It could say "no" but never "why", so a
user staring at an unselectable 13 GB row had no way to learn that AI history
is report-only, or that a Docker scope was off, or that privileged operations
were disabled.

## Decision

1. **`blocked_reason: Option<BlockReason>` replaces the boolean**, plus
   `blocked_detail: Option<String>` carrying the sentence to show. The reasons
   are a closed set — `Retention`, `AiReadOnly`, `PrivilegedRequired`,
   `DockerScope`, `Protected` — because a reason with no enforcement point is
   just a label.

2. **The scan stamps it; the engine refuses what is stamped.** Both call the
   same function, `devclean_core::policy::block_reason_for`. The scanner runs
   it during aggregation (so the UI can explain a resource before the user
   tries to select it) and the engine runs it before touching anything.

3. **`block_reason_for` lives in `core`** and takes plain values rather than a
   `Provider`, so the scanner and the cleanup engine can both use it without a
   dependency cycle.

4. **The engine also re-derives from the live policy.** Settings can change
   between the scan that stamped nothing and the cleanup call, and a stale
   "allowed" must not become a route past the gate.

5. **Authorisation outranks age.** If a resource is both AI report-only and
   inside the retention window, the reason shown is `AiReadOnly` — telling
   someone about a 30-day window when they have not enabled AI cleanup at all
   would hide the setting that actually matters.

## Consequences

- `PolicyInputs` is now a parameter of `ScanManager::scan_all` / `scan_one`,
  built in `commands.rs` from `Settings`. A new gate therefore has one place
  to be added.
- `Settings.ai_cleanup_enabled` (default `false`) makes AI conversation
  history report-only. Its resources stay in `ScanSummary` with sizes — the
  user should see what exists — but cannot be selected until they opt in.
- `HistoryEntry.sizing` records which sizing convention produced a row's byte
  figures. `#[serde(default)]` → `Apparent`, which is what every line written
  before v0.8.0 actually used, and the history page labels such runs
  "before 0.1.0" rather than silently comparing them with allocated totals.
- Measured after the fix, on this machine: with a 10-year retention policy,
  **106 of 106 resources** are blocked (96 by retention, 10 by AI report-only),
  holding back 69.6 GB. Before the fix the same policy blocked **zero**.

## Rejected alternatives

- **Keep the boolean and add the missing assignment.** Rejected: it would fix
  this instance and leave the shape that allowed it — a control that can be
  set, persisted and displayed without any code reading it.
- **Stamp only in the engine.** Rejected: the UI then cannot explain a
  resource until the user clicks it, and `retention_blocked_bytes` goes back
  to being decorative.
- **Derive the reason only from live settings, never stamp it.** Rejected: the
  scanner would have to re-derive per render, and the summary could not report
  held-back bytes without the same logic in a third place.
