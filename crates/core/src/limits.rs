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
