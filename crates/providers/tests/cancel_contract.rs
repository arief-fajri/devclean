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
    walk_tree_rules, AndroidProvider, CargoRegistryProvider, ClaudeProvider, DockerProvider,
    DockerPrunedProvider, FlutterProvider, GlobalCacheProvider, GoProvider, GoVendorProvider,
    GradleCacheProvider, NodeModulesProvider, NpmCacheProvider, OpencodeDbProvider,
    OpencodeFsProvider, OwnScanner, PodsProvider, PythonCacheProvider, RustTargetProvider,
    TreeRule, XcodeDataProvider,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Fixture paths must be unique per call: these tests run in parallel
/// threads of one process, and a shared path means one test deletes the
/// tree while another walks it.
static FIXTURE_SEQ: AtomicUsize = AtomicUsize::new(0);

/// A scan that must never touch the filesystem: every provider checks the
/// flag before its first probe or measurement.
fn cancelled_scan(p: &dyn OwnScanner) -> Vec<devclean_core::Resource> {
    let cancel = AtomicBool::new(true);
    let roots: Vec<PathBuf> = Vec::new();
    p.scan(&roots, &cancel)
}

/// One fixture holding a match for every tree rule, so the tree half of the
/// contract runs against real work a scan WOULD report without the flag —
/// an empty root would make the assertion vacuous.
fn tree_fixture(tag: &str) -> PathBuf {
    let n = FIXTURE_SEQ.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!(
        "devclean-cancel-tree-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("proj/node_modules")).unwrap();
    std::fs::write(root.join("proj/package.json"), b"{}").unwrap();
    std::fs::write(root.join("proj/node_modules/x.js"), b"x").unwrap();
    std::fs::create_dir_all(root.join("rs/target")).unwrap();
    std::fs::write(root.join("rs/Cargo.toml"), b"[package]").unwrap();
    std::fs::write(root.join("rs/target/app"), b"x").unwrap();
    std::fs::create_dir_all(root.join("py/__pycache__")).unwrap();
    std::fs::write(root.join("py/__pycache__/m.pyc"), b"x").unwrap();
    // Wave D3: one match per new rule, each in its own project so no two
    // rules contend for one directory.
    std::fs::create_dir_all(root.join("gomod/vendor")).unwrap();
    std::fs::write(root.join("gomod/go.mod"), b"module example.com/p\n").unwrap();
    std::fs::write(root.join("gomod/vendor/x.go"), b"x").unwrap();
    std::fs::create_dir_all(root.join("fl/build")).unwrap();
    std::fs::write(root.join("fl/pubspec.yaml"), b"name: fl\n").unwrap();
    std::fs::write(root.join("fl/build/app.dill"), b"x").unwrap();
    std::fs::create_dir_all(root.join("andr/build")).unwrap();
    std::fs::write(root.join("andr/build.gradle"), b"// module\n").unwrap();
    std::fs::write(root.join("andr/build/out.apk"), b"x").unwrap();
    std::fs::create_dir_all(root.join("iospods/Pods")).unwrap();
    std::fs::write(root.join("iospods/Podfile"), b"platform :ios\n").unwrap();
    std::fs::write(root.join("iospods/Pods/x.swift"), b"x").unwrap();
    root
}

/// The tree rules ride the shared walk; the walk checks the flag before it
/// offers the first entry, so a pre-set cancel reports nothing.
fn tree_scan(rule: Arc<dyn TreeRule>, cancel: bool) -> Vec<devclean_core::Resource> {
    let root = tree_fixture(rule.id());
    let flag = AtomicBool::new(cancel);
    let groups = walk_tree_rules(std::slice::from_ref(&root), &[rule], &flag);
    let _ = std::fs::remove_dir_all(&root);
    groups.into_iter().flatten().collect()
}

/// Without the flag the fixture DOES match — proof the cancelled rows below
/// are not passing because the fixture is empty.
#[test]
fn the_tree_cancel_fixture_actually_matches() {
    for rule in [
        Arc::new(NodeModulesProvider) as Arc<dyn TreeRule>,
        Arc::new(RustTargetProvider),
        Arc::new(PythonCacheProvider),
        Arc::new(GoVendorProvider),
        Arc::new(FlutterProvider),
        Arc::new(AndroidProvider),
        Arc::new(PodsProvider),
    ] {
        assert_eq!(tree_scan(rule, false).len(), 1, "fixture must match once");
    }
}

#[test]
fn node_modules_stops_when_cancelled() {
    assert!(tree_scan(Arc::new(NodeModulesProvider), true).is_empty());
}

#[test]
fn rust_target_stops_when_cancelled() {
    assert!(tree_scan(Arc::new(RustTargetProvider), true).is_empty());
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
    assert!(tree_scan(Arc::new(PythonCacheProvider), true).is_empty());
}

#[test]
fn go_vendor_stops_when_cancelled() {
    assert!(tree_scan(Arc::new(GoVendorProvider), true).is_empty());
}

#[test]
fn flutter_stops_when_cancelled() {
    assert!(tree_scan(Arc::new(FlutterProvider), true).is_empty());
}

#[test]
fn android_stops_when_cancelled() {
    assert!(tree_scan(Arc::new(AndroidProvider), true).is_empty());
}

#[test]
fn pods_stops_when_cancelled() {
    assert!(tree_scan(Arc::new(PodsProvider), true).is_empty());
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
