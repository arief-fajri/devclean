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
