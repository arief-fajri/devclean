//! G-SCAN-02's contract, per provider: a PRE-set cancel flag must make
//! `scan` return empty.
//!
//! This is the regression net for the hole Wave C closed: six fixed-path
//! providers (`global_cache`, `cargo_registry`, `gradle_cache`,
//! `xcode_data`, `claude`, `opencode_fs`) used to measure their directories
//! with the uncancellable `dir_size_detail` and ignored the flag entirely,
//! so a cancel during a large `~/.cargo/registry` walk settled only when
//! the walk finished — never in <1s. One test per provider, so a broken
//! row fails one test rather than the whole contract.
//!
//! The assertion is machine-independent: whatever the directories hold,
//! a cancelled scan reports nothing.

use devclean_providers::{
    CargoRegistryProvider, ClaudeProvider, DockerProvider, DockerPrunedProvider,
    GlobalCacheProvider, GoProvider, GradleCacheProvider, NodeModulesProvider, NpmCacheProvider,
    OpencodeDbProvider, OpencodeFsProvider, Provider, PythonCacheProvider, RustTargetProvider,
    XcodeDataProvider,
};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

/// A scan that must never touch the filesystem: every provider checks the
/// flag before its first probe or measurement.
fn cancelled_scan(p: &dyn Provider) -> Vec<devclean_core::Resource> {
    let cancel = AtomicBool::new(true);
    let roots: Vec<PathBuf> = Vec::new();
    p.scan(&roots, &cancel)
}

#[test]
fn node_modules_stops_when_cancelled() {
    assert!(cancelled_scan(&NodeModulesProvider).is_empty());
}

#[test]
fn rust_target_stops_when_cancelled() {
    assert!(cancelled_scan(&RustTargetProvider).is_empty());
}

#[test]
fn go_cache_stops_when_cancelled() {
    assert!(cancelled_scan(&GoProvider).is_empty());
}

#[test]
fn docker_stops_when_cancelled() {
    assert!(cancelled_scan(&DockerProvider).is_empty());
}

#[test]
fn docker_pruned_stops_when_cancelled() {
    assert!(cancelled_scan(&DockerPrunedProvider).is_empty());
}

#[test]
fn global_cache_stops_when_cancelled() {
    assert!(cancelled_scan(&GlobalCacheProvider).is_empty());
}

#[test]
fn npm_cache_stops_when_cancelled() {
    assert!(cancelled_scan(&NpmCacheProvider).is_empty());
}

#[test]
fn python_cache_stops_when_cancelled() {
    assert!(cancelled_scan(&PythonCacheProvider).is_empty());
}

#[test]
fn cargo_registry_stops_when_cancelled() {
    assert!(cancelled_scan(&CargoRegistryProvider).is_empty());
}

#[test]
fn gradle_cache_stops_when_cancelled() {
    assert!(cancelled_scan(&GradleCacheProvider).is_empty());
}

#[test]
fn xcode_data_stops_when_cancelled() {
    assert!(cancelled_scan(&XcodeDataProvider).is_empty());
}

#[test]
fn opencode_fs_stops_when_cancelled() {
    assert!(cancelled_scan(&OpencodeFsProvider).is_empty());
}

#[test]
fn claude_stops_when_cancelled() {
    assert!(cancelled_scan(&ClaudeProvider).is_empty());
}

#[test]
fn opencode_db_stops_when_cancelled() {
    assert!(cancelled_scan(&OpencodeDbProvider).is_empty());
}
