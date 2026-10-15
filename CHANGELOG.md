# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Two version numbers move independently:

- the **product** version (`Cargo.toml`, `tauri.conf.json`) — what a user installs
- the **contract** version (`docs/reference/tauri-contract.md`) — the IPC surface
  between the bundled frontend and backend, which always ship together

A contract bump therefore does not imply a product release, and vice versa.

## [Unreleased]

Nothing yet. Add a line here when a change is visible to a user; it moves into a
version section at release time.

## [0.1.0] — 2026-10-15

First public release. macOS only; see
[Platform support](README.md#platform-support).

Everything below landed before this tag, so it is listed as one release rather
than as the sequence of milestones it was built in.

### Added

#### Scanning

- 14 providers: `node_modules`, Rust `target/`, Go build and module caches,
  Docker build cache, Docker images and volumes, system and tool caches,
  npm/pnpm/yarn caches, Python caches, Cargo registry, Gradle caches, Xcode
  DerivedData, opencode data, opencode history, Claude data.
- **Scan roots are detected, not typed.** Defaults cover the usual dev folders
  and **Settings → Scan → Detect folders** discovers the rest by walking `$HOME`
  at most two levels, never descending into a cache. A `settings.json` written
  before this release with an empty root list is re-seeded from the defaults.
- Concurrent scanning (max 4 providers at once) with per-provider timeouts,
  cooperative cancellation that settles in under a second, and a live progress
  bar that names the provider currently running.
- Per-provider re-scan, so refreshing one card costs that provider's scan
  rather than a full sweep.
- Scan cache on disk for an instant paint at startup, bound to the scan roots
  and rejected when they change.

#### Safety

- `SafetyValidator`: a path is refused if it is protected, inside a protected
  path, or an ancestor of one — and if it is outside the configured scan roots.
  Protected: `/`, `~/Documents`, `~/Desktop`, `~/Downloads`, `~/Pictures`,
  `~/Videos`, `~/.ssh`, `~/.gnupg`, `~/.config`.
- **Opaque resource ids.** The frontend never supplies a path; it sends an id
  the backend resolves. A path is never a capability held by the UI.
- **Backend-enforced confirmation.** `preview_cleanup` issues a token bound to
  the exact id set with a 300-second TTL; `cleanup` refuses without a valid one
  while confirmation is on. The gate is enforced by the backend, not by the UI
  choosing to ask.
- **`blocked_reason`** on every resource that cannot be cleaned, stamped at scan
  time and refused by the engine from the same function — so the explanation the
  user reads is the one that is enforced. Reasons: `retention`, `ai_read_only`,
  `privileged_required`, `docker_scope`, `protected`.
- **Age-based retention.** Keep anything younger than N days out of cleanup.
  Held-back resources stay listed with their sizes and cannot be selected.
- Per-resource results: success, partial, failed or cancelled, each with its own
  measured byte figures and reason.

#### Deleting

- **Move to Trash by default.** Recoverable; `permanent` is an explicit opt-out.
  Cross-volume moves fall back to copy-then-remove, and the original is removed
  only after the copy fully succeeds.
- Configurable Docker scope — build cache only by default; images and volumes
  additionally require the privileged opt-in.
- Vendor commands where an ecosystem ships one: `go clean`, `npm cache clean`,
  `pnpm store prune`, `docker builder prune`.
- AI conversation history is **report-only** until explicitly enabled. Session
  transcripts and agent state are not regenerable, so the shipped default is to
  show them and refuse to delete them.
- Config and credentials (`history.jsonl`, `settings.json`, `auth.json`, …) are
  never prune targets, whatever pattern matches them.

#### Measuring

- **Sizes are allocated blocks, not file lengths.** `size_bytes` is what
  deleting a resource actually frees. Hard-linked inodes are counted once per
  artifact.
- Sparse files report both figures — `8.6 GB (appears as 460 GB)`.
- **`freed_bytes` is measured, never asserted.** It is `before − after` for
  every cleanup path, including subprocess-backed ones. When the after-state
  cannot be measured the result reports `0` and says why.

#### Interface

- Tauri 2 + Svelte 5 desktop app: sidebar with Overview, Resources, History and
  Settings pages.
- Read-only menu bar surface: reclaimable total, kept bytes, top providers,
  stale count and a low-disk warning, plus navigation. It never starts a
  cleanup.
- Cleanup history, grouped by run and day, with per-resource outcomes.
- Dark and light themes.

#### Project

- 195 Rust tests and 139 frontend tests.
- CI on macOS **and** Ubuntu, with a pinned toolchain.
- Guardrails with enforcement mechanisms rather than prose — including ones
  enforced by tests that read the source: pages may not call the cleanup API,
  the tray menu is a fixed allowlist, every button has a pointer cursor, and a
  platform claim must be backed by CI.
- Decision records under [`docs/decisions/`](docs/decisions/).

### Fixed

Four controls were found claiming enforcement they did not have. All four are
fixed, each with a test that fails if the mechanism disappears.

- **`Settings.retention_days` had no effect on any cleanup.** The backing field
  was never set to `false` anywhere in production code, so the setting
  persisted, displayed, and did nothing. A 10-year policy now blocks every
  resource it should; before, the same policy blocked none.
- **`freed_bytes` for subprocess-backed cleanup always equalled the requested
  figure.** The after-state was measured on a path that is never a directory, so
  it always read zero. A prune that reclaimed 3 GB reported 10 GB.
- **Docker build cache read as zero on any machine that had one.** Docker
  appends a reclaimable percentage — `11.95GB (83%)` — which the size parser
  rejected, and a rejected parse became `0`.
- **Rust `target/` over-reported by 145%.** Cargo hard-links 92% of its build
  artifacts, and each link was counted as a separate file. `target/` read
  26.6 GB against `du`'s 10.8 GB.
- Scan progress could run backwards and report `N/N` with work still pending,
  because the counter reported a provider's position in the list rather than how
  many had finished.
- A `settings.json` with an empty `scan_roots` array left the app permanently
  blind to every project cache, because the stored list shadowed the defaults.
- Six node_modules trees now match `du` to `+0.0%`.

### Security

- No known vulnerabilities at release.
- Report suspected ones privately: see [SECURITY.md](SECURITY.md).

[0.1.0]: https://github.com/arief-fajri/devclean/releases/tag/v0.1.0
