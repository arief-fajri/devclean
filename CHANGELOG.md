# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Two version numbers move independently:

- the **product** version (`Cargo.toml`, `tauri.conf.json`) — what a user installs
- the **contract** version (`docs/reference/tauri-contract.md`) — the IPC surface between the bundled frontend and backend, which always ship together

A contract bump therefore does not imply a product release, and vice versa.

## [Unreleased]

### Added

- **Wails builds are covered.** A project with `wails.json` at its root now reports `build/bin/` (the compiled app — `Caution`, a full rebuild) and `frontend/dist/` (the frontend bundle — `Safe`) as their own `wails` provider. The rest of `build/` (`appicon.png`, platform templates) is committed config and is deliberately not touched. Decision record: [wails-coverage](docs/decisions/wails-coverage.md).

### Fixed

- **Tools installed in the normal places are no longer invisible to the packaged app.** A Finder-launched app inherits launchd's `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), which has neither `/usr/local/bin` (Docker Desktop) nor `/opt/homebrew/bin` (Homebrew's `go`). Every CLI-backed provider therefore returned empty — Docker, Docker images & volumes, and the Go caches never appeared, with no error anywhere. `system::augment_process_path()` now runs before the builder spawns anything and appends `/etc/paths`, `/etc/paths.d/*`, a fixed list of well-known locations (`/usr/local/bin`, `/opt/homebrew/bin`, `~/.cargo/bin`, `~/.bun/bin`, …), and the highest installed nvm Node. No shell is executed and the list is fixed server-side (G-SAFE-01). See [security.md](docs/security.md).
- **Choosing "Build cache + unused images" now saves.** Rust serialized `DockerPruneScope::BuildCacheImages` as `build-cache-images` (the derived `kebab-case` name) while the contract and `tauri.ts` spell it `build-cache+images`; picking that option could not round-trip through `update_settings`. The variant now uses the contract's spelling, accepts the old spelling as an alias so existing `settings.json` files still load, and has a regression test for all four values.
- **A Docker scope narrower than an action needs is refused.** `block_reason_for` only checked "scope off" plus the privileged opt-in, so with Privileged operations on, a `build-cache` selection still let `docker_pruned` (which declares `all`) run `docker image prune -af` and `docker volume prune -f`. The selected scope is now enforced as a minimum: the row carries the new `docker_scope_required` reason (detail names the scope to choose) and the cleanup engine refuses it through the same function. `docker_scope` keeps its meaning — the scope is `off` — so the two hints no longer contradict each other.

## [1.0.1] — 2026-10-31

### Fixed

- **The safety validator now sees through symlinks.** `SafetyValidator::validate_path` additionally checks the canonical (symlink-resolved) form of the target — best-effort up to the nearest existing ancestor — so a scan root or path component that is a symlink into protected territory (`~/Dev → ~/Documents/Dev`) is refused before `remove_dir_all` runs, instead of slipping past the purely lexical check and deleting real files under `~/Documents`. Regression tests cover scenarios A, B and C from the v1.0.0 verification, plus an engine end-to-end test proving the neighbour file survives a refused run. See [security.md](docs/security.md).
- **The "no undo" warning is where it can still stop the click.** `Permanent deletion has no undo` now appears on the confirmation screen, not only on the done screen after the bytes are gone (the done screen keeps it). The acknowledgement checkbox says "deleted permanently and cannot be undone" instead of the false-for-Dangerous "can be recreated", and a selection containing `dangerous` items demands a second, consequence-spelling acknowledgement before the Clean button unlocks.

### Changed

- **The "regenerable" claims are honest.** `system-model.md`'s desired outcome and the permanent-only ADR no longer say *everything* deleted is regenerable — almost everything is; the exceptions are the items classified **Dangerous** (Xcode Archives with shipped dSYMs, opencode conversation history, Docker local volumes), which carry their own consequence copy and the second acknowledgement. The ADR's "dialogs state it out loud" now matches the code.

## [1.0.0] — 2026-10-24

### Added

- **The deduplicated figure reaches the dashboard (Wave E3).** The Overview's summary panel now shows `unique_bytes` as its own line when the backend can still measure it, and says nothing when it cannot (a targeted rescan, a post-cleanup prune, an older cache) — never a stale number. The panel carries the value and never recomputes it.
- **Store and component tests with a real DOM (Wave E2).** 120 new tests over the seven Svelte stores, `SelectionBar`, `Sidebar`, `ResourceDrawer`, `SummaryPanel` and the shell itself — including behavioural proof that the shell ignores every hotkey while the confirmation flow is open (G-UI-03), which until now was only a source scan. The rig is happy-dom + `@testing-library/*`; the suite runs on the Ubuntu CI runner unchanged.

### Changed

- **One source of truth for every gate (Wave E3, C-4).** The cleanup engine's `refusal_for` had been re-implementing `core::policy::block_reason_for` — the same gates, the same strings, in two places, which is the failure class that lets a UI control and a backend refusal drift apart (I11). The engine now assembles `PolicyInputs` and calls the one function the scan stamps with; order, precedence and messages are identical, and the engine's only additions are what the core cannot know (the scan-time stamp, and that git protection is a scan-time classification). Retention and the Docker/AI/browser gates now have exactly one implementation each.
- **Releases become universal (arm64 + x86_64) from 1.0.0.** `release.yml` cross-compiles the Intel half on the Apple Silicon runner and `lipo` merges it; one dmg and one updater tarball serve both architectures, and `latest.json` lists them under both `darwin-aarch64` and `darwin-x86_64` (the updater looks up its own architecture). Releases through 0.4.0 were arm64-only; the in-app-updates ADR records why the deferral was reversed for 1.0. Cost: roughly double build time.
- **The contract document was brought back in sync with the bridge (Wave E3).** `tauri-contract.md`'s Types block had lost `Settings.ai_cleanup_enabled`, `Settings.check_updates_automatically`, `HistoryEntry.sizing` and `CleanupRun.pre_allocated_sizing` — every one shipped and present in `tauri.ts`, none in the document that calls itself the single source of truth. All four are restored (plus legacy `HistoryEntry.moved_to_trash?` for pre-1.0 dev-build history), the duplicated behavioural-note number is fixed (the list is now 1–17 with its cross-references updated), and `contract-sync.test.ts` diffs the document's type block against `tauri.ts` so the two cannot drift apart again (its honest scope: document ↔ bridge, not Rust).
- **Permanent-only cleanup.** Every cleanup deletes outright via `remove_dir_all` (or the provider's fixed tool command) and `freed_bytes` always reports the measured `before − after` reclaim — there is no Trash copy and no delete-mode choice. The safety net is the re-measured preview plus backend-enforced confirmation, and the dialogs state `Permanent deletion has no undo` out loud. Old `settings.json` files containing `"delete_mode"` still load; dev-build history lines with `"moved_to_trash": true` still parse and render as `archived in Trash (pre-1.0)`. See [permanent-only-cleanup](docs/decisions/permanent-only-cleanup.md).
- **The menu bar colours all 21 providers (Wave E3, K-1).** The seven Wave D ids — `go_vendor`, `flutter`, `android`, `pods`, `browser_temp`, `llm_models`, `ruby` — had tray chips falling through to the grey fallback; they now carry their `providerMeta.ts` colours and ride the package/disc glyph families. `xcode_data` stays grey on both sides because zinc *is* its colour.
- **Documentation says the current state (Wave E4).** `guardrails.md` is down to definitions plus their enforcement column — the "why these exist" table and the G-CI-03 narrative moved here with the project's history, where the roadmap said they belonged. `roadmap.md` is rewritten for the post-1.0 horizon (notarization, the real upgrade-chain test, a Linux bundle, Windows's recorded entry cost, the three ecosystems still missing) instead of carrying shipped work. Three ADRs record the decisions 1.0 needed: [unsigned-for-1.0](docs/decisions/unsigned-for-1.0.md), [one-point-oh](docs/decisions/one-point-oh.md) (breaking contract change ⇒ 2.0) and [windows-deferred](docs/decisions/windows-deferred.md).
  - *From the trimmed `guardrails.md` — why each rule exists (the history the file no longer carries):*

    | Was | Now | What caught it |
    |---|---|---|
    | "G-UI-01 enforced by code" | enforced by `preview_token` in the engine | the old enforcement was a `tracing::debug!` |
    | the UI shell revamp could put a cleanup call one click from the list (G-UI-02/03) | the menu-bar id allowlist + the page-level scan of `routes/*.svelte` | reading the diff |
    | the tray icon painted a white square (G-UI-04) | the asset is decoded and measured in CI, and generated from one geometry | the earlier PNG was written as greyscale *without* an alpha channel, so macOS painted the whole frame; nothing in Rust, the build or a screenshot diff caught it |
    | `retention_eligible` was never set to `false` in production, so `Settings.retention_days` had no effect and `retention_blocked_bytes` was permanently 0 | `blocked_reason` stamped at scan time by `devclean_core::policy::block_reason_for`, refused by the engine from the same function | a 10-year policy now blocks every resource it should; before, it blocked none |
    | `freed_bytes` for command-backed cleanup always equalled `requested` | `before − after`, both measured; unmeasurable reports 0 with a reason | `runcommand_freed_bytes_is_measured_not_assumed` |
    | the frontend tests — including G-UI-02/03/05 — were never run by CI | the `frontend` job runs `npm test` | `platform-claim.test.ts` asserts it |

    G-CI-01 and G-AI-01 were dropped once while the guardrails file was restructured, and nothing noticed — a guardrail document is not covered by a test that fails when an entry disappears. They were restored; it is the same failure class as everything else in the table: a control that existed only as prose stopped existing, and only a failing CI job found it.

  - *From the trimmed `guardrails.md` — G-CI-03 in practice.* Twice a test passed locally and failed on a clean CI runner because it asserted something about *the machine* while its name claimed to assert something about the product: `disk::the_app_data_directory_really_is_on_the_home_volume` really asserted "this machine has run DevClean" (`app_data_dir()` does not exist until it has), and `policy_evidence::retention_and_ai_gates_now_reach_the_scan` really asserted "this machine has AI history" (a runner has never run opencode or Claude). How to satisfy the rule: build the thing you are testing (`ScanManager::with_config` takes any `Provider`, so a stub exercises the pipeline without a real directory), compare against a fixture you created in the test, and when a path may not exist yet resolve to the nearest existing ancestor. Git fixtures pass every setting explicitly — a runner has no global git identity, and a user's `core.excludesFile`/`core.fileMode` change what `check-ignore` and `status` report (`git -c core.excludesFile=/dev/null -c core.fileMode=false check-ignore <path>`); the live example is in `crates/scanner/tests/git_classification.rs`.

### Security

- **A Content Security Policy is now enforced in production builds.** The webview runs with `default-src 'self'; connect-src ipc: http://ipc.localhost; style-src 'self' 'unsafe-inline'` — no remote script/style/assets and no network reach except the Tauri IPC channel, so a hypothetical injection cannot load code or exfiltrate. The one relaxation, `style-src 'unsafe-inline'`, is required by the inline `style="width: …%"` progress bars; script keeps no inline or `eval` allowance. Dev is deliberately left unlocked (`devCsp: null`) so Vite HMR works; the policy is verified against a production build. This is hardening, not the security boundary — every destructive gate still lives in Rust. The same bundle config pins the webview floor to **macOS 12.0 (Monterey)**. See [docs/security.md](docs/security.md).
- **Two TLS advisories in the updater's chain patched (1.0 release audit).** `cargo audit` flagged `rustls-webpki 0.103.15` (RUSTSEC-2026-0382) and `webpki-root-certs 1.0.9` (RUSTSEC-2026-0383) — both could accept certificates chaining to distrusted intermediates, and both sit directly under `tauri-plugin-updater → reqwest → rustls`, i.e. the connection that fetches every update (G-UPD-01). The lockfile now carries `0.103.16` and `1.0.11`; `cargo audit` reports **0 vulnerabilities**. `npm audit`: 0 vulnerabilities. Remaining cargo-audit output is six informational advisories recorded rather than silenced: unmaintained `proc-macro-error`, `zbus_names`, `zvariant`, `zvariant_derive` (transitive via Tauri's Linux stack) and unsound `glib::VariantStrIter` (Linux only) and `tokio::AsyncFd` (constructors this project does not use).

### Removed

- **Dead retention and tree-walk code.** `devclean_cleanup::apply_retention` duplicated what `core::policy::block_reason_for` already stamps at scan time and was called only by its own test; `TreeRuleSpec.extra_prune_dirs` was declared, set to `&[]` in all eight providers, and never read by the walk. Both are gone — retention still gates exactly as before, through the policy that both the scan and the engine share.

## [0.4.0] — 2026-10-23

### Added

- **Seven new providers (Wave D coverage): 14 → 21.** Four ride the shared tree walk — `go_vendor` (`vendor/` beside `go.mod`), `flutter` (`.dart_tool/` and `build/` beside `pubspec.yaml`), `android` (`build/`, `.gradle/`, `.cxx/` beside any gradle marker — plain Android projects and the Android half of React Native) and `pods` (`Pods/` beside `Podfile`, the iOS half) — one traversal however many rules, so a scan still walks each root once. Three scan fixed home paths: `llm_models` (Ollama, Hugging Face, LM Studio, llamafile model stores), `ruby` (`~/.gem`, `~/.bundle/cache` — never the `~/.bundle` root, which can hold registry credentials) and `browser_temp`. Every path choice and every deliberate exclusion is recorded in [docs/decisions/wave-d-coverage.md](docs/decisions/wave-d-coverage.md).
- **Browser temp files, gated by a live liveness probe.** Chromium-family browsers (Chrome, Chromium, Edge, Brave, Arc, Vivaldi, Opera) clone their whole `.app` bundle into the system temp area when codesign re-signs a helper, and crashed or automated sessions leave the copies behind — 92 copies / 156 GB observed in the wild. `browser_temp` reports one row per browser, and a **confirmed-running** browser blocks it: the scan stamps `blocked_reason: "browser_running"` and the cleanup path **re-probes and refuses live**, so a scan that went stale cannot delete the clone of a browser that started meanwhile. A failed probe never blocks but floors the row's safety to `caution`. APFS copy-on-write means the real reclaim is often far smaller than the size shown — `freed_bytes` is measured and the row says so. Contract v0.13.0; decision record: [docs/decisions/browser-temp-probe.md](docs/decisions/browser-temp-probe.md).
- **`ScanSummary.unique_bytes` — the deduplicated figure.** After a full scan, what deleting every reported artifact would actually return: an inode hard-linked into two artifacts counted once, while `total_bytes` keeps summing rows as before. It becomes `null` whenever the figure can no longer be measured (targeted rescan, post-cleanup prune, an older cache); the frontend carries it and never recomputes it.

### Changed

- **Existing providers cover more.** The node rule now matches `.next`, `.turbo`, `.svelte-kit` and `.expo` beside `package.json`; the Python rule matches `.ruff_cache`, `.tox` and `__pypackages__`; `global_cache` grows CocoaPods, Yarn, pip, deno, the Dart pub cache and the Dart analysis server cache. **Xcode data becomes four rows** — DerivedData (review), **Archives (dangerous: the dSYMs of shipped builds die with it)**, iOS DeviceSupport and the Simulator caches (review) — with Simulator *devices* deliberately excluded: simulator data is not a cache. Contract v0.13.0.

## [0.3.2] — 2026-10-22

### Changed

- **Settings tabs: Scan → Providers.** The first Settings tab now lists only the providers, each with its on/off toggle and what the last scan found. *Scan roots* and the *Ignore list* moved to **Appearance** (after Theme), where a path-shaped setting fits better; the explanatory *Rows* section there is gone — the Paths toggle it described lives in the Resources toolbar and was never configurable from Settings anyway. Internal tab id, panel ids and test ids follow the rename (`scan` → `providers`). No command, event or response changed; the contract stays at v0.12.0.

## [0.3.1] — 2026-10-22

### Fixed

- **The selection bar no longer sits behind page content.** With items selected, the bottom bar (count, safe/review split, *Review & clean selected*) could be painted under the sticky provider headers while the page scrolled, so the actions it carries were covered. It now takes its own stacking level, below the resource drawer and the cleanup dialog but above the page.

## [0.3.0] — 2026-10-16

### Added

- **In-app updates.** Settings gains an **Updates** tab: the running version, Check for updates, release notes, a download progress bar, Download & install, and Restart to apply. Update bundles are signed with a minisign key and verified by the backend **before** they are installed, over HTTPS; nothing is downloaded until you ask. Installing is refused while a scan or cleanup is running, and it never restarts the app on its own. An optional "check at startup" toggle is **off by default** — with it off, DevClean makes no outbound request until you press Check. Contract v0.12.0; decision record: [docs/decisions/in-app-updates.md](docs/decisions/in-app-updates.md).

### Changed

- **Project trees are scanned once (Wave C).** `node_modules`, Rust `target/` and the Python caches used to walk every scan root apiece and then re-walk each artifact for its size. They now contribute rules to one shared traversal that matches and sizes in the same pass, so a scan of a large dev tree does a single walk instead of three plus per-match re-walks (isolated benchmark: **2.1×** the old three walks, **1.6×** the old three walks plus re-measurement). The reported resources and their sizes are unchanged — the pre-refactor output is pinned by an equivalence suite. The only visible difference is event timing: those three providers now emit `scan_provider_started` together and `scan_progress` together, instead of each firing as its own walk finished. Contract v0.11.0; decision record: [docs/decisions/single-pass-walk.md](docs/decisions/single-pass-walk.md).

### Fixed

- **A symlinked Rust `target/` is no longer reported.** Directory matching during the shared walk is `lstat`-only, so a `target/` that is itself a symlink is skipped rather than followed — consistent with G-SCAN-03 (symlinks are never followed), which the old `target.is_dir()` check quietly violated.

## [0.2.0] — 2026-10-16

### Added

- **Git-aware classification (Wave B).** Activity is now the composite of an artifact's mtime, its repository's last commit and its project directory's mtime, surfaced as an `activity` tier (`active`/`recent`/`stale`/`dormant`). A project committed to daily stops looking dormant, so age-based retention no longer offers an active project's cache.
- **A repository's uncommitted work is protected.** An artifact that a repository tracks — or a new file it does not yet track — is refused while that repository is dirty (`blocked_reason: "protected"`). A gitignored artifact such as `node_modules` stays cleanable even then, so the tool is still usable in the repositories people work in. Paths outside any repository, and machines without git, behave exactly as before.

### Changed

- **A new brand mark.** The app icon, favicon, in-app header and menu bar icon now use the DevClean leaf outline (`assets/devclean-leaf.svg`) instead of the old emerald sprout tile. Purely visual: no command, event or response changed.

- **Cleanup no longer freezes the app.** Finishing a cleanup used to trigger a full scan of every provider, which dimmed the list and disabled selection. The cleaned items now leave the list immediately, and only the providers that were touched are re-scanned, in the background; the other providers keep their last-known figures and are flagged as a partial re-scan.

### Fixed

- **Scan-time probes carry a deadline (G-REL-05).** `run_tool`, `go_env`, `pgrep` and the new git calls all run through one helper that kills the child after 30s, instead of spawning with no timeout.

## [0.1.0] — 2026-10-15

First public release. macOS only; see [Platform support](README.md#platform-support).

Everything below landed before this tag, so it is listed as one release rather than as the sequence of milestones it was built in.

### Added

#### Scanning

- 14 providers: `node_modules`, Rust `target/`, Go build and module caches, Docker build cache, Docker images and volumes, system and tool caches, npm/pnpm/yarn caches, Python caches, Cargo registry, Gradle caches, Xcode DerivedData, opencode data, opencode history, Claude data.
- **Scan roots are detected, not typed.** Defaults cover the usual dev folders and **Settings → Scan → Detect folders** discovers the rest by walking `$HOME` at most two levels, never descending into a cache. A `settings.json` written before this release with an empty root list is re-seeded from the defaults.
- Concurrent scanning (max 4 providers at once) with per-provider timeouts, cooperative cancellation that settles in under a second, and a live progress bar that names the provider currently running.
- Per-provider re-scan, so refreshing one card costs that provider's scan rather than a full sweep.
- Scan cache on disk for an instant paint at startup, bound to the scan roots and rejected when they change.

#### Safety

- `SafetyValidator`: a path is refused if it is protected, inside a protected path, or an ancestor of one — and if it is outside the configured scan roots. Protected: `/`, `~/Documents`, `~/Desktop`, `~/Downloads`, `~/Pictures`, `~/Videos`, `~/.ssh`, `~/.gnupg`, `~/.config`.
- **Opaque resource ids.** The frontend never supplies a path; it sends an id the backend resolves. A path is never a capability held by the UI.
- **Backend-enforced confirmation.** `preview_cleanup` issues a token bound to the exact id set with a 300-second TTL; `cleanup` refuses without a valid one while confirmation is on. The gate is enforced by the backend, not by the UI choosing to ask.
- **`blocked_reason`** on every resource that cannot be cleaned, stamped at scan time and refused by the engine from the same function — so the explanation the user reads is the one that is enforced. Reasons: `retention`, `ai_read_only`, `privileged_required`, `docker_scope`, `protected`.
- **Age-based retention.** Keep anything younger than N days out of cleanup. Held-back resources stay listed with their sizes and cannot be selected.
- Per-resource results: success, partial, failed or cancelled, each with its own measured byte figures and reason.

#### Deleting

- **Move to Trash by default.** Recoverable; `permanent` is an explicit opt-out. Cross-volume moves fall back to copy-then-remove, and the original is removed only after the copy fully succeeds.
- Configurable Docker scope — build cache only by default; images and volumes additionally require the privileged opt-in.
- Vendor commands where an ecosystem ships one: `go clean`, `npm cache clean`, `pnpm store prune`, `docker builder prune`.
- AI conversation history is **report-only** until explicitly enabled. Session transcripts and agent state are not regenerable, so the shipped default is to show them and refuse to delete them.
- Config and credentials (`history.jsonl`, `settings.json`, `auth.json`, …) are never prune targets, whatever pattern matches them.

#### Measuring

- **Sizes are allocated blocks, not file lengths.** `size_bytes` is what deleting a resource actually frees. Hard-linked inodes are counted once per artifact.
- Sparse files report both figures — `8.6 GB (appears as 460 GB)`.
- **`freed_bytes` is measured, never asserted.** It is `before − after` for every cleanup path, including subprocess-backed ones. When the after-state cannot be measured the result reports `0` and says why.

#### Interface

- Tauri 2 + Svelte 5 desktop app: sidebar with Overview, Resources, History and Settings pages.
- Read-only menu bar surface: reclaimable total, kept bytes, top providers, stale count and a low-disk warning, plus navigation. It never starts a cleanup.
- Cleanup history, grouped by run and day, with per-resource outcomes.
- Dark and light themes.

#### Project

- 195 Rust tests and 139 frontend tests.
- CI on macOS **and** Ubuntu, with a pinned toolchain.
- Guardrails with enforcement mechanisms rather than prose — including ones enforced by tests that read the source: pages may not call the cleanup API, the tray menu is a fixed allowlist, every button has a pointer cursor, and a platform claim must be backed by CI.
- Decision records under [`docs/decisions/`](docs/decisions/).

### Fixed

Four controls were found claiming enforcement they did not have. All four are fixed, each with a test that fails if the mechanism disappears.

- **`Settings.retention_days` had no effect on any cleanup.** The backing field was never set to `false` anywhere in production code, so the setting persisted, displayed, and did nothing. A 10-year policy now blocks every resource it should; before, the same policy blocked none.
- **`freed_bytes` for subprocess-backed cleanup always equalled the requested figure.** The after-state was measured on a path that is never a directory, so it always read zero. A prune that reclaimed 3 GB reported 10 GB.
- **Docker build cache read as zero on any machine that had one.** Docker appends a reclaimable percentage — `11.95GB (83%)` — which the size parser rejected, and a rejected parse became `0`.
- **Rust `target/` over-reported by 145%.** Cargo hard-links 92% of its build artifacts, and each link was counted as a separate file. `target/` read 26.6 GB against `du`'s 10.8 GB.
- Scan progress could run backwards and report `N/N` with work still pending, because the counter reported a provider's position in the list rather than how many had finished.
- A `settings.json` with an empty `scan_roots` array left the app permanently blind to every project cache, because the stored list shadowed the defaults.
- Six node_modules trees now match `du` to `+0.0%`.

### Security

- No known vulnerabilities at release.
- Report suspected ones privately: see [SECURITY.md](SECURITY.md).

[1.0.0]: https://github.com/arief-fajri/devclean/releases/tag/v1.0.0
[0.4.0]: https://github.com/arief-fajri/devclean/releases/tag/v0.4.0
[0.3.2]: https://github.com/arief-fajri/devclean/releases/tag/v0.3.2
[0.3.1]: https://github.com/arief-fajri/devclean/releases/tag/v0.3.1
[0.3.0]: https://github.com/arief-fajri/devclean/releases/tag/v0.3.0
[0.2.0]: https://github.com/arief-fajri/devclean/releases/tag/v0.2.0
[0.1.0]: https://github.com/arief-fajri/devclean/releases/tag/v0.1.0
