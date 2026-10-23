# Roadmap

What is left, not what happened. Shipped work is in [CHANGELOG.md](../../CHANGELOG.md); the reasoning behind it is in [docs/decisions](../decisions/).

## Where we are

Product **0.4.0** is Wave D — coverage: seven providers added (14 → 21), the existing node/Python/global-cache/Xcode rules widened, a live liveness gate for Chromium-family browser temp files (`BlockReason::BrowserRunning`, re-probed at cleanup), and the whole-scan dedup figure `ScanSummary.unique_bytes`. The contract sits at **v0.13.0**. The reasoning lives in [wave-d-coverage.md](../decisions/wave-d-coverage.md) and [browser-temp-probe.md](../decisions/browser-temp-probe.md); what shipped is in [CHANGELOG.md](../../CHANGELOG.md).

Two numbers move independently — the product version a user installs, and the contract version of the IPC surface between the bundled frontend and backend. A contract bump does not imply a product release, and the reverse is also true. At 1.0 they are aligned deliberately.

## Version ladder

| Wave | Content | Product | Contract |
|---|---|---|---|
| **A** | First public release | `0.1.0` | `v0.8.0` |
| — | Cache-first startup (shipped as a fix) | `0.1.0` | `v0.9.0` |
| **B** | Git-aware classification | `0.2.0` | `v0.10.0` |
| **C** | Single-pass walk | `0.3.0` | `v0.11.0` |
| — | In-app updates | `0.3.0` | `v0.12.0` |
| **D** | Coverage (shipped — see below) | `0.4.0` | `v0.13.0` |
| **E** | Stabilisation and documentation | `1.0.0` | `v1.0.0` |

Rules that apply to every wave:

- Every new gate goes through `devclean_core::policy::block_reason_for`. Four controls in this project have already shipped documented and inert; none of them took that route, and no test caught it because the test asserted the control existed rather than that it fired.
- Caller-visible changes update [docs/reference/tauri-contract.md](../reference/tauri-contract.md) and add a regression test.
- A decision someone could reasonably have made differently gets an ADR in `docs/decisions/`.
- The fixture and README screenshots are refreshed when the dashboard's shape changes.
- Full gate before a version bump: `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, and the frontend's `check`, `test` and `build`.
- Changing what may be cleaned needs a human decision before implementation, not after.

---

## Wave D — Coverage (SHIPPED — 0.4.0 / v0.13.0)

Executed in seven commits, one per stage (D1–D7), on top of the Wave C shared
walk it depended on. Providers **14 → 21**: `browser_temp`, `llm_models`,
`ruby`, `go_vendor`, `flutter`, `android`, `pods`; existing `node_modules`,
`python_cache`, `global_cache` and `xcode_data` rules widened. The two
Level B decisions approved 2026-10-20 both shipped as designed:

- **browser_temp gate — design A.** `BlockReason::BrowserRunning` stamped
  through `block_reason_for` at scan time, re-probed and refused live through
  `refusal_for` at cleanup; a failed probe never blocks but floors the safety
  to `caution` (`NotRunning` is the only state that yields `safe`). Enforced
  by **G-SAFE-07** and its pin tests.
- **Cross-artifact dedup — additive field.** `ScanSummary.unique_bytes`:
  `Some` after a full scan, `None` when the figure can no longer be measured;
  no `CACHE_VERSION` bump, frontend never recomputes it.

Nothing was deferred to E. The per-stage plan that drove the work is trimmed
here on purpose (a plan is an ephemeral artifact); the decisions live in
[wave-d-coverage.md](../decisions/wave-d-coverage.md) and
[browser-temp-probe.md](../decisions/browser-temp-probe.md), the shipped
behaviour in [CHANGELOG.md](../../CHANGELOG.md) and
[tauri-contract.md](../reference/tauri-contract.md) (v0.13.0, with each
behaviour's named regression test).

---

## Wave E — Stabilisation and documentation

The gate for `1.0.0`. Two halves: close what is open, then make the documentation true.

### Close the recorded gaps

- **Component tests for the shell** — Sidebar navigation, page switching, drawer focus, cleanup-dialog keyboard blocking. Needs a DOM environment.
- **XDG Trash** — `trash.rs` currently states Linux is "deliberately not implemented". It becomes implemented but **not verified**: `default_trash_dir()` turns platform-conditional (`$XDG_DATA_HOME/Trash` on Linux, with `files/` and `info/` and one `.trashinfo` per artifact, percent-encoded `Path=` and a `DeletionDate=`), and the per-mount `.Trash-$UID` case is best-effort. CI can test the deterministic part — the bytes of the `.trashinfo` and the encoding — but not the file appearing in a real GNOME or KDE Trash. That is stated rather than hidden, and the platform table is not affected: Linux still builds and tests, and nothing is shipped for it.
- **G-REL-05** — closed in Wave B; verify it did not regress.
- ~~**Cross-artifact deduplication**~~ — shipped in Wave D (0.4.0). The bullet stays struck through only long enough to say so; it goes with the next edit of this file.

### Documentation

The repository is public, so the documentation is the product as much as the code is. Before 1.0:

- `roadmap.md` is rewritten again for the post-1.0 horizon, so it stays forward-looking rather than accumulating shipped work.
- Executed plan documents are removed. A plan is an ephemeral artifact: once it ships, the plan goes and its decisions live in `docs/decisions/`.
- `guardrails.md` is reduced to current state; its history belongs in `CHANGELOG.md`.
- Every document is checked against reality — the contract, the contract changelog, the versions in `Cargo.toml` and `tauri.conf.json`, the support table, and the commands in `developing.md`.

### The 1.0 commitment

`1.0.0` promises IPC stability; after it, a breaking change needs `2.0`. The contract reaches `1.0.0` with the product. If that promise cannot be honoured, staying at `0.x` is more honest than making it.

Decisions required before 1.0: whether Windows is supported or permanently unsupported, and whether signing and notarization are pursued (they need an Apple Developer account; until then the first-launch steps are documented).

**Level B** — the stability commitment and the XDG Trash change both alter the cleanup path.

---

## Accepted limitations

Recorded rather than hidden. Each names why it is not fixed.

- **G-SAFE-06 — the confirmation token is a workflow gate, not authentication.** The frontend runs in the same process tree and can read any token it is handed, so a fully compromised frontend can mint its own preview. What it does buy is real: an id set that was never shown to the user is refused. What it cannot buy needs an out-of-process UI. Accepted and documented rather than chased, because full authentication against a frontend in the same process tree is a false promise without OS-level isolation.
- **No bundle is shipped for Linux.** The workspace builds and the full suite runs on Ubuntu in CI, which exercises the Unix-specific code on a second OS, but no `.deb`, `.rpm` or AppImage is produced and the platform is not supported.
- **In-app updates are arm64-only.** CI builds on Apple Silicon, so `latest.json` carries only a `darwin-aarch64` entry; an Intel Mac would not receive an update. Adding a universal build (and a `darwin-x86_64` entry) is deferred.
- **The updater is built with `default-features = false`.** The published `tauri-plugin-updater` depends on a `zip` version that is fully yanked on crates.io; the `zip` feature is Windows-only and macOS uses its own `tar`+`flate2`, so it is disabled. Revisit when upstream pins `zip 9`.
- **The first updater-enabled release must be installed by hand.** An older build has no updater to receive it; automatic updates work from the next release onward.

## Non-goals

Things this project will deliberately not do, so they are not re-litigated:

- **Permanent deletion by default.** `trash` is the default; `permanent` is an explicit opt-out. A cleanup tool that cannot be undone is one mistake away from losing a week of work.
- **Deleting AI conversation history by default.** It is reported with its size but stays unselectable until the user enables it in Settings.
- **Deleting a database it cannot take exclusively.** No `VACUUM` on a file another process holds open.
- **Touching config or credentials.** `history.jsonl`, `settings.json`, `auth.json` and friends are hard-preserved, whatever pattern matches them.
- **Reporting a number it did not measure.** `freed_bytes` is `before − after`; when the after-state is unmeasurable the result says `0` and why.
- **Claiming a platform it does not test.** See the support table in the README.
