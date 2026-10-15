//! Concurrent, cancellable scanning. Providers run in parallel behind a
//! bounded semaphore (guardrail G-SCAN-01); cancellation is cooperative and
//! must settle in <1s (G-SCAN-02).

pub mod cache;
pub mod manager;

pub use cache::{load_cached_scan, load_cached_summary, save_cached_summary};
pub use manager::ScanManager;
