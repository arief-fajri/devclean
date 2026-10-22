# Roadmap

What is left, not what happened. Shipped work is in [CHANGELOG.md](../../CHANGELOG.md); the reasoning behind it is in [docs/decisions](../decisions/).

## Where we are

Product **0.3.2** is 0.3.1 — the Settings restructure: the first tab is **Providers** (the list only), while scan roots and the ignore list live under **Appearance**. Before that, 0.3.0 — the single-pass project-tree walk (project caches are found and sized in one traversal) and **in-app updates**: Settings checks the release channel, and a signed, verified update can be downloaded and installed without leaving the app — plus a selection-bar layering fix. The contract sits at **v0.12.0**.

Two numbers move independently — the product version a user installs, and the contract version of the IPC surface between the bundled frontend and backend. A contract bump does not imply a product release, and the reverse is also true. At 1.0 they are aligned deliberately.

## Version ladder

| Wave | Content | Product | Contract |
|---|---|---|---|
| **A** | First public release | `0.1.0` | `v0.8.0` |
| — | Cache-first startup (shipped as a fix) | `0.1.0` | `v0.9.0` |
| **B** | Git-aware classification | `0.2.0` | `v0.10.0` |
| **C** | Single-pass walk | `0.3.0` | `v0.11.0` |
| — | In-app updates | `0.3.0` | `v0.12.0` |
| — | Selection-bar layering fix (no contract change) | `0.3.1` | `v0.12.0` |
| — | Settings tabs restructure (no contract change) | `0.3.2` | `v0.12.0` |
| **D** | Coverage | `0.4.0` | `v0.13.0` |
| **E** | Stabilisation and documentation | `1.0.0` | `v1.0.0` |

Rules that apply to every wave:

- Every new gate goes through `devclean_core::policy::block_reason_for`. Four controls in this project have already shipped documented and inert; none of them took that route, and no test caught it because the test asserted the control existed rather than that it fired.
- Caller-visible changes update [docs/reference/tauri-contract.md](../reference/tauri-contract.md) and add a regression test.
- A decision someone could reasonably have made differently gets an ADR in `docs/decisions/`.
- The fixture and README screenshots are refreshed when the dashboard's shape changes.
- Full gate before a version bump: `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, and the frontend's `check`, `test` and `build`.
- Changing what may be cleaned needs a human decision before implementation, not after.

---

## Wave D — Coverage

Depends on the shared walk Wave C shipped in 0.3.0. Without it this would add roughly eight full traversals. Staged D1–D7, one commit per stage; ships as **0.4.0** / contract **v0.13.0**. Providers 14 → 21.

### Level B decisions (approved 2026-10-20, before implementation)

- **browser_temp gate — design A.** A confirmed-running browser stamps a new `BlockReason::BrowserRunning` through `devclean_core::policy::block_reason_for` at scan time, and the cleanup path **re-probes** and refuses live through `CleanupEngine::refusal_for`, so a scan that went stale cannot delete the cache of a browser that started meanwhile. A failed probe (`ProcState::Unknown`) never blocks, but floors the safety to `caution` — never `safe`. `NotRunning` is the only state that yields `safe`. Enforced by the new G-SAFE-07 and tests that pin `Unknown` away from `safe`.
- **Cross-artifact dedup — additive field.** `ScanSummary.unique_bytes: Option<u64>`: `Some` after a full scan, `None` whenever the figure can no longer be measured (targeted rescan, post-cleanup prune, an older cache). `total_bytes` keeps meaning "sum of the rows". No `CACHE_VERSION` bump: absence is the honest default, and an old cache loading `None` is correct rather than wrong.
- **Nothing in this wave is deferred to E.** The Wave E carry-over bullet stays only as a contingency.

### The shape of the coverage

Seven new ids. Every path list is fixed server-side; exclusions are decisions, not omissions.

| id | shape | what it finds | safety |
|---|---|---|---|
| `browser_temp` | own | zombie Chromium-family code-sign clones under `/var/folders/…/X/*.code_sign_clone` (one resource per browser, from a fixed bundle-prefix → `pgrep` pattern table: Chrome, Chromium, Edge, Brave, Arc, Vivaldi, Opera). Observed at 92 copies / 156 GB in the wild — though APFS copy-on-write means the real reclaim is often ~30× smaller than `du` claims; `freed_bytes` is measured, and `impact` says so. | `NotRunning` → `safe`; `Unknown` → `caution` (floor); `Running` → blocked |
| `llm_models` | own | `~/.ollama/models` (never `~/.ollama` itself — keys and config live there), `~/.cache/huggingface/hub`, `~/.lmstudio/models`, `~/.cache/lm-studio/models` (legacy), `~/.cache/llamafile` | `caution` |
| `ruby` | own | `~/.gem`, `~/.bundle/cache` (never the `~/.bundle` root — its config can hold credentials) | `caution` |
| `flutter` | tree | `build/` and `.dart_tool/` next to a `pubspec.yaml` | `build` → `caution`, `.dart_tool` → `safe` |
| `android` | tree | `build/`, `.gradle/`, `.cxx/` next to any gradle marker (`build.gradle(.kts)`, `settings.gradle(.kts)`) — covers plain Android projects and the Android half of React Native | `caution` |
| `pods` | tree | `Pods/` next to a `Podfile` — CocoaPods, the iOS half of "React Native" | `caution` |
| `go_vendor` | tree | `vendor/` next to a `go.mod` | `caution` |

React Native coverage is therefore `.expo` under `node_modules` (D2), `android` (Gradle), and `pods` (CocoaPods). Shared caches that are not project artifacts — the Dart pub cache among them — ride `global_cache` rather than a tool-specific id.

### D1 — rule enablers

- `TreeRuleSpec.sibling_file` becomes `sibling_files` (any-of). The gradle markers and `Podfile` are one-of-many; the three existing rules change shape only, match semantics otherwise untouched.
- Equivalence suite green, shared-walk bench re-run (no regression).

### D2 — expanded rules on existing ids

- node `dir_names` `+ .next`, `.turbo`, `.svelte-kit`, `.expo` (sibling `package.json`) — `safe`.
- Python `CACHE_DIRS` `+ .ruff_cache`, `.tox`, `__pypackages__` — `safe`.
- `global_cache` targets `+` CocoaPods, Yarn, pip, deno, Dart pub cache (`~/.pub-cache`), Dart server cache — all `caution` (we do not own their internals).
- `xcode_data` restructured into a target list: DerivedData (`caution`), **Archives (`dangerous` — the dSYMs of shipped builds die with them)**, iOS DeviceSupport (`caution`), `CoreSimulator/Caches` (`caution`). `CoreSimulator/Devices` is deliberately excluded: simulator data is not a cache, and no gate can make it one.
- Caller-visible (new rows under existing ids): behaviour note in the contract changelog + the equivalence `expected_set` grows as the regression test.

### D3 — the four tree providers

- `go_vendor`, `flutter`, `android`, `pods`: spec, `materialize` with the per-resource safety above, prefixes `gv` / `fl` / `ad` / `pd` in `short_prefix`, `providerMeta` entries + `KNOWN` list, a row each in the cancel contract, and the shared tree fixture extended so those rows are not vacuous.

### D4 — browser_temp (the hazard)

- Discovery: `std::env::temp_dir().parent()` + `X`, then the fixed bundle-prefix table; one resource per browser app dir; the clone root goes through `extra_allowed_roots`, protected-path rules still apply.
- `safety_for(ProcState)` as a pure function, plus G-SAFE-07 in `guardrails.md`: *a failed liveness probe fails toward more caution, never less* — with the pin test that `Unknown` never yields `safe` and the floor property that it never lowers.
- `BlockReason::BrowserRunning` + `label()`, `PolicyInputs.browser_running`, gate placed with the other authorization gates (before retention), mirrored in `refusal_for`, badge/hint in `blocked.ts`, and the probe wired in `commands.rs` **twice** — when the scan policy is built and when the cleanup policy is built.
- ADR: `docs/decisions/browser-temp-probe.md`.

### D5 — llm_models + ruby

- Fixed target lists in the `global_cache` style. A running Ollama daemon changes `detail`/`impact` text only (the precedent `claude` sets); it is not a new gate.
- ADR: `docs/decisions/wave-d-coverage.md` — every path choice and exclusion in this wave: `vendor/bundle` (its marker is two levels up; matching the whole `vendor/` would offer committed code), AVD userdata and system images, the `~/.bundle` root, and env-var model dirs (`OLLAMA_MODELS`, `HF_HOME`) as a recorded non-goal.

### D6 — cross-artifact deduplication

- Inode keys ride `DirSize` into `Resource` behind `#[serde(skip)]` (internal only; no scanner-context refactor, no shared mutex — providers keep their own sets, `aggregate()` folds them first-wins into `unique_bytes`). Per-artifact figures are unchanged.
- Invalidation rules: full scan → `Some`; targeted rescan, post-cleanup prune, unknown provenance → `None`. The frontend carries the value and never recomputes it (it cannot: the keys never leave the backend).
- Tests: `unique_bytes <= total_bytes`, an old cache loads `None`, a targeted merge clears it, and a new guard that no two resources in one scan share a path. Bench: no walk regression.

### D7 — release 0.4.0 / contract v0.13.0

- Contract changelog entry: Providers 14 → 21, `+BrowserRunning`, `+ScanSummary.unique_bytes`, the browser-temp behaviour, the expanded-rules behaviour; each with a named regression test.
- Fixture (`demo/make-fixture.sh`) gains small trees for every new rule, README screenshots refreshed per `docs/operations/screenshots.md` (G-PUB-04).
- `CHANGELOG.md`, product bump (`Cargo.toml`, `tauri.conf.json`, README), ADR status lines, "Where we are" rewritten, and **this plan trimmed** — it is an executed plan document once it ships.
- Full gate: `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, frontend `check` / `test` / `build`.

Contract `v0.13.0`, additive: new provider ids in `get_providers` and in scan results, `BlockReason::BrowserRunning`, `ScanSummary.unique_bytes`.

**Level B** — new providers change what is offered for deletion, and `browser_temp` is destructive. Both halves approved above, before implementation.

---

## Wave E — Stabilisation and documentation

The gate for `1.0.0`. Two halves: close what is open, then make the documentation true.

### Close the recorded gaps

- **Component tests for the shell** — Sidebar navigation, page switching, drawer focus, cleanup-dialog keyboard blocking. Needs a DOM environment.
- **XDG Trash** — `trash.rs` currently states Linux is "deliberately not implemented". It becomes implemented but **not verified**: `default_trash_dir()` turns platform-conditional (`$XDG_DATA_HOME/Trash` on Linux, with `files/` and `info/` and one `.trashinfo` per artifact, percent-encoded `Path=` and a `DeletionDate=`), and the per-mount `.Trash-$UID` case is best-effort. CI can test the deterministic part — the bytes of the `.trashinfo` and the encoding — but not the file appearing in a real GNOME or KDE Trash. That is stated rather than hidden, and the platform table is not affected: Linux still builds and tests, and nothing is shipped for it.
- **G-REL-05** — closed in Wave B; verify it did not regress.
- **Cross-artifact deduplication** — if not finished in Wave D.

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
