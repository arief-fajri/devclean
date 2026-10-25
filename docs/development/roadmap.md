# Roadmap

What is left, not what happened. Shipped work is in [CHANGELOG.md](../../CHANGELOG.md); the reasoning behind it is in [docs/decisions](../decisions/).

## Where we are

Product **1.0.0**, contract **v1.0.0** — aligned, as the Wave E ladder promised. The contract now breaks only at 2.0 ([one-point-oh.md](../decisions/one-point-oh.md)), and the document that defines it is diffed against the bridge by `contract-sync.test.ts`.

1.0 was the stabilisation release: permanent-only cleanup (Trash removed pre-release so reclaimed space returns immediately), behavioural tests for the seven stores and the shell itself (a real DOM, G-UI-03 proved by firing keystrokes rather than matching strings), the contract document reconciled with the code that ships it, and dead gates removed so retention and Docker refusal each have exactly one source of truth. The record is in [CHANGELOG.md](../../CHANGELOG.md).

Two version numbers still move independently — the product version a user installs and the contract version of the IPC surface. They always ship together; the contract just cannot break before 2.0.

## What is next

Ordered by what unlocks the most users first.

### Release hardening

- **Notarization.** 1.0 ships unsigned by decision ([unsigned-for-1.0.md](../decisions/unsigned-for-1.0.md)); the one-time Gatekeeper step is documented rather than hidden. An Apple Developer Program membership, CI secrets and a stapling step in `release.yml` are all mechanical — none of it is a design question, which is exactly why it was not allowed to gate 1.0.
- **Prove the upgrade chain end to end.** Install 0.4.0 → Check for updates → 1.0.0 → restart → scan. CI cannot simulate an old installed copy; this is the one test only a real machine can run, and it validates the whole minisign updater path (G-UPD-01..04) against a release people actually have.

### Coverage

- **Swift Package Manager (`.build`), Bazel caches, Gradle wrapper distributions** — the three names on the README's "not yet" list, each one a `TreeRule` or a fixed-path scanner on the existing machinery.
- **Deeper shell component tests.** The rig now renders components; the cleanup dialog's `running` and `done` phases, and the selection bar under a live scan, are the next behaviours worth pinning.
- **`commands.rs` has almost no direct tests** (only preview-deadline and cache-seed). The engine is heavily tested underneath it; integration coverage that crosses the command layer would close the gap the 1.0 audit recorded as an accepted risk.

### Platforms

- **Linux bundle (deb / AppImage).** CI already builds and tests the whole workspace on Ubuntu — but nothing is shipped, so the support table says "not shipped" and stays honest. Before Linux can be called supported: build the bundle and verify it end to end on a real machine.
- **Windows — deferred, not rejected.** [windows-deferred.md](../decisions/windows-deferred.md) records the decision and the entry cost (cfg-gate the Unix-only disk paths, an `msi`/`nsis` target, a `windows-latest` runner). The support row changes only after the runner exists — `platform-claim.test.ts` enforces that order.
- **Intel builds are already done**: releases are universal (arm64 + x86_64) since 1.0.0 ([in-app-updates.md](../decisions/in-app-updates.md)).

### Security horizon

- **G-SAFE-06 stays an accepted limitation.** The preview token is a workflow gate, not an authentication boundary: a fully compromised frontend can mint its own preview and call `cleanup`. The real fix is an out-of-process frontend, which is only worth its cost if the threat model changes — this app's threat is a wrong click, not a hostile renderer. Revisit if that ever stops being true.

## Rules that apply to every change

- Every new gate goes through `devclean_core::policy::block_reason_for`. Four controls in this project have already shipped documented and inert; none of them took that route, and no test caught it because the test asserted the control existed rather than that it fired.
- Caller-visible changes update [docs/reference/tauri-contract.md](../reference/tauri-contract.md) and add a regression test. `contract-sync.test.ts` enforces the Types block against `tauri.ts`; behavioural notes are contract too.
- A decision someone could reasonably have made differently gets an ADR in `docs/decisions/`.
- The fixture and README screenshots are refreshed when the dashboard's shape changes.
- Full gate before a version bump: `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, and the frontend's `check`, `test` and `build`.
- Changing what may be cleaned needs a human decision before implementation, not after.
