//! Explicit operation limits (G-SCAN-02, G-REL-01). No external operation
//! may block forever: every provider scan and every cleanup subprocess
//! runs under one of these timeouts.

use std::time::Duration;

/// Maximum wall time for a single provider's scan. Exceeded → the provider
/// contributes nothing this round and a warning is logged (never a generic
/// failure, G-REL-02).
pub const PROVIDER_SCAN_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum wall time for one cleanup subprocess (`go clean`, `docker
/// builder prune`, …). Exceeded → the child is killed and the resource
/// reports `Failed` with a timeout message (G-REL-02).
pub const CLEANUP_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum wall time for a single provider **probe** subprocess — a
/// `git log`, a `go env`, a `pgrep`, a tool that reports a cache path.
///
/// G-REL-05: before Wave B these had no deadline at all, so a hung probe
/// parked a thread. A probe is expected to answer in milliseconds; a probe
/// that has not answered in 30s is not going to.
pub const PROVIDER_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum wall time for the cleanup **preview** — re-measuring every selected
/// path before the confirmation appears.
///
/// This one was missing for v0.7.2. `preview_cleanup` walks the selected
/// paths on a blocking thread, so a large selection could keep the dialog
/// waiting indefinitely, with the UI showing nothing. It is a UI-facing
/// operation, so G-REL-01 ("no external operation may block forever") applies
/// to it exactly as it applies to a scan or a subprocess. Exceeded → the
/// command fails with an explanation and nothing has been deleted.
pub const PREVIEW_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum wall time for the in-app **update check** — one HTTPS request to
/// the release channel (G-UPD-02). A check that has not answered in 15s is
/// not going to, and the Settings button must not spin forever (G-REL-01).
pub const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(15);

/// Maximum wall time for an update **download**. Generous: a release bundle
/// is tens of megabytes and the user is waiting on their own connection, but
/// it still may not run forever (G-REL-01).
pub const UPDATE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
