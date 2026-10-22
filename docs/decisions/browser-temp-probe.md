# ADR: the browser-temp probe, and why one `pgrep` can gate a delete

**Status:** accepted (product 0.4.0, contract v0.13.0) **Fixes:** nothing — new capability. The 156 GB of zombie code-sign clones this ships a provider for were, until now, invisible to the product.

## Context

Chromium-family browsers (Chrome, Chromium, Edge, Brave, Arc, Vivaldi, Opera) clone their entire `.app` bundle into the boot-cleaned temp area when `codesign` re-signs a helper:

```text
/var/folders/<cluster>/<user>/X/<bundle-id>.code_sign_clone/code_sign_clone.XXXXXX/
```

A clean quit removes the active clone. A crash, an automation session (Playwright, ChromeDriver, Codex Computer Use) or an unclean update leaves it behind, and the leftovers accumulate without any tool reporting them: 92 copies / 156 GB observed by `du` on one machine; public Chromium and Electron bug trackers report the same shape at 14 GB, 35 GB, 62 GB and 183 GB.

The clones of a **running** browser are live files. A cleaning tool that deletes them out from under the process is worse than one that ignores them — so a naive `rm -rf` provider was never an option, and doing nothing left the largest single reclaim on the machine unaddressed. The design question was: what evidence is strong enough to delete, and when must the delete be refused?

## Decision

**1. A fixed bundle-prefix table, and nothing else.** Seven browsers, each with a bundle-id prefix (names its clone dir) and a `pgrep -f` regex (names its processes), both `&'static`. No globbing, no discovery of "whatever looks like a browser" (G-SAFE-01/02). Electron apps leak identically (`com.openai.codex.code_sign_clone`); they are deliberately NOT in the table — reporting an app we cannot name, probe or explain would be worse than not reporting it.

**2. Design A: scan-time stamp + cleanup-time re-probe.** A confirmed-running browser stamps `BlockReason::BrowserRunning` through `devclean_core::policy::block_reason_for` at scan time; the cleanup path **re-probes** and refuses live through `CleanupEngine::refusal_for`, so a scan that went stale cannot delete the clone of a browser that started meanwhile. Both halves go through the one gate function (I11): the reason the user reads is the reason the engine enforces.

**3. `Unknown` never blocks, but floors the safety (G-SAFE-07).** A failed probe (`pgrep` absent, timed out, sandboxed out) is not evidence of absence — the `ai_guard` module documents why: a sandboxed process cannot see the process table and answers as if nothing were running, which is the dangerous direction. So `NotRunning` is the ONLY state that yields `safe`; `Unknown` floors to `caution`; `Running` blocks outright. The floor is a pure function (`safety_for` / `floor_safety`) with pin tests, never a branch buried in the scan.

**4. The gate is provider-declared, not id-matched.** `Provider::held_while_browsers_run()` (default `false`, only `browser_temp` answers `true`) arms the gate at scan time (the scanner ANDs it with the probe before `block_reason_for`) and at cleanup time (`refusal_for` asks the provider, never the id — P1). A running Chrome therefore cannot block node_modules; the same probe value reaching a provider without the declaration is inert, and a test pins that.

**5. The probe is one `pgrep` per browser, bounded, and never under a lock.** `PROVIDER_COMMAND_TIMEOUT` (30 s) via `run_with_deadline` (G-REL-05); the scan-side probe runs on the blocking pool BEFORE the settings mutex is taken, the cleanup-side probe inside the existing `spawn_blocking`. A cancelled scan returns before the first `pgrep` (G-SCAN-02).

## Consequences

- The largest reclaim on a long-lived Mac — often bigger than all node_modules put together — becomes visible, gated and cleanable.
- The safety floor means a machine where `pgrep` is unavailable shows the rows as `caution` (Review) rather than `safe`: the product admits what it cannot know instead of guessing "nothing is running".
- APFS copy-on-write means `du` overstates the reclaim (shared extents counted once per clone; one report saw 14 GB by `du` free 4 GB, another 65 GB by `du` free ~724 MB). `freed_bytes` is measured (I12), and the `impact` sentence says the real reclaim is often far smaller than the size shown — the scan number never implies otherwise.
- The table is a maintenance surface: a browser that renames its bundle or process path drops out silently (it is simply not reported). That is the honest failure — an unreported directory — rather than a wrong one.

## Rejected alternatives

- **Provider-stamped `blocked_reason` (bypassing `block_reason_for`).** Mechanically possible (`stamp_blocked_reason` preserves provider-set reasons) but every gate must flow through the one function, or the reason the user reads and the reason the engine enforces can drift (I11's whole point).
- **Per-browser precision (block only the running browser's rows).** Attractive — Edge's clones could stay selectable while Chrome runs — but `PolicyInputs.browser_running` is one bool by approved design; per-browser gating would need a set of running prefixes carried into the policy and re-derived in the engine. Conservative over-blocking (any browser running → all browser-temp rows held) was chosen over that complexity; a running browser also re-creates its own clones, so the blocked set is exactly the set that is mutating.
- **`Unknown` refuses.** Fails closed, but a hung `pgrep` would permanently disable the feature and teach users to ignore the badge. The floor gets the caution without the uselessness.
- **Reporting every `*.code_sign_clone` dir.** Would surface Electron apps the product cannot probe or name. Recorded as a non-goal; the table can grow deliberately.
- **`sysinfo`-style in-process enumeration.** A new native dependency for what one bounded `pgrep` answers; absence of `pgrep` is already modeled as `Unknown`, which is the honest answer.
