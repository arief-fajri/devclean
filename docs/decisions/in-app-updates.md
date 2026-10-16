# ADR: in-app updates, signed and opt-in

**Status:** accepted (product 0.3.0, contract v0.12.0)

## Context

DevClean ships as an unsigned macOS `.app` in a `.dmg` from GitHub Releases. Every release is a fresh manual download: a user who installed 0.2.0 has no way to learn that 0.3.0 exists, let alone install it, short of watching the repository.

Two facts shape the design:

1. **The app is unsigned.** There is no Apple Developer certificate, so macOS refuses a browser-downloaded build until the user opens it once by hand. That is documented in the README and is not what this ADR changes.
2. **An updater is a network capability and a security boundary.** Whatever downloads and installs code into the app must verify it came from us, must not be tricked into accepting a plaintext or forged payload, and must never restart the app out from under a running cleanup.

Tauri v2 ships an updater (`tauri-plugin-updater`) with a **minisign** signature chain that is independent of Apple code signing: the private key signs each release bundle, the public key is committed in `tauri.conf.json`, and the plugin verifies the payload **before** installing it. Verification cannot be disabled.

## Decision

Add the Tauri updater, driven from Rust, surfaced as a **"Updates" tab in Settings**, with the startup check **off by default**.

1. **Distribution.** The endpoint is the repository's own release channel:
   `https://github.com/arief-fajri/devclean/releases/latest/download/latest.json`.
   The release workflow generates `latest.json` from the built `.app.tar.gz` and its `.sig`, pointing at the tag's own asset so the manifest and the binary cannot drift. `arm64` only, matching what CI builds.

2. **Signing.** The public key is committed (`plugins.updater.pubkey`); the private key and its password are repo secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). `createUpdaterArtifacts` is turned on **only in the release workflow** (`--config` override), never in `tauri.conf.json`, so a local `cargo tauri build` never needs the key. `dangerousInsecureTransportProtocol` is never set — TLS is enforced.

3. **Rust, not the JS plugin.** Three commands and one event (`get_update_status`, `check_for_update`, `install_update`, `restart_app`; `update_status`) keep `lib/tauri.ts` the only bridge and keep every capability in the contract. The frontend never imports `@tauri-apps/plugin-updater`.

4. **Bounded and opt-in.** The check is bounded by `UPDATE_CHECK_TIMEOUT` (15s) and the download by `UPDATE_DOWNLOAD_TIMEOUT` (300s). The startup check runs only when `Settings.check_updates_automatically` is on (default **off**), and it is spawned, never awaited, so the window never waits on the network.

5. **Install and restart are separate.** `install_update` downloads and installs, then reports `ready_to_restart`; the user presses "Restart to apply", which is `restart_app`. The user sees that the install succeeded before the window disappears.

6. **No restart mid-cleanup.** `run_scan` and `cleanup` hold an RAII `BusyGuard`; `install_update` refuses while it is set. A restart during a deletion is the one genuinely dangerous interaction, so it is refused rather than queued.

### A note on the `zip` feature

`tauri-plugin-updater`'s published crate depends on `zip ^4` (Windows only), and every `zip` version from 2.x through 8.x is currently yanked on crates.io, so a default-features build cannot resolve. The `zip` feature only pulls the **Windows** archive path and the **Linux** optional `tar`/`flate2`; **macOS uses its own non-optional `tar` + `flate2`**. We therefore build the plugin with `default-features = false, features = ["rustls-tls", "system-proxy"]`. macOS is fully functional; the Linux/Windows install paths compile but return "unsupported" at runtime, which is irrelevant to a macOS-only app. This is a deliberate, documented workaround and should be revisited once upstream releases a version pinned to `zip 9`.

## Consequences

- A user on any release **after** the first updater-enabled one can check, download, install and restart without leaving the app.
- The **first** updater-enabled release still has to be installed by hand: an older build has no updater to receive it. Auto-update works from the next release onward.
- Updates are cryptographically verified even though the app is unsigned. The swap is performed by the app process, so no quarantine attribute is applied and Gatekeeper does not re-prompt; first-install friction is unchanged.
- Losing the minisign private key permanently strands every installed copy: they can never verify a new release. The key must be backed up outside CI.
- The app now has a routine outbound capability, so it is **opt-in**. A user who never enables it makes no request.
- Contract v0.12.0: four commands, one event, one type, one settings field. Cache format unchanged.

## Rejected alternatives

- **Check-only, open the release page.** Rejected for this app: it leaves the user to find, download and de-quarantine a `.dmg` by hand — the exact friction the feature exists to remove.
- **The JavaScript updater plugin.** Rejected: it adds a second bridge outside `lib/tauri.ts`, needs `updater:*` capabilities, and cannot express "refuse while a cleanup is running" as cleanly as the Rust side.
- **Auto-check on by default.** Rejected: DevClean's stance elsewhere is restraint (`scan_automatically` is off, nothing spawns on the boot path), and a silent request to GitHub on every launch is not the user's choice to make for them.
- **Auto-restart after install.** Rejected: a window that vanishes without warning is a worse experience than one button.
- **Universal (arm64 + x86_64) builds.** Deferred: CI builds on Apple Silicon; Intel users would need a second target and a `darwin-x86_64` manifest entry.
- **`includeUpdaterJson` from tauri-action.** Rejected: it attaches the manifest to a release the action creates, and this workflow deliberately keeps release creation in a separate reviewed `publish` job.
