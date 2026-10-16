//! Wave C characterization: the set of resources the three project-tree
//! providers report over one fixture tree.
//!
//! This test is the refactor's safety net. `scan_tree_providers` below is
//! the ONLY part that changes across Wave C (three independent walks → one
//! shared walk); the fixture and every assertion are meant to stay
//! byte-identical so the equivalence is reviewable as a diff of one function.
//!
//! Sizes are filesystem-dependent (allocated blocks), so the assertions are
//! structural (which paths, which quirks) plus size *invariants* that pin the
//! measurement semantics from `allocated-vs-apparent.md`: hard links counted
//! once per artifact, sparse files surfacing `apparent_bytes`, empty-dir
//! rules per provider.

use devclean_core::Resource;
use devclean_providers::{
    NodeModulesProvider, OwnScanner, PythonCacheProvider, RustTargetProvider,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

// ---------------------------------------------------------------- fixtures

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("devclean-equiv-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

/// Deterministic pseudo-random bytes: distinct per seed, so two 1 MiB files
/// cannot be collapsed by a compressing filesystem and the hard-link
/// assertion measures inode dedup, not content dedup.
fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).max(1);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state & 0xFF) as u8
        })
        .collect()
}

fn build_fixture(root: &Path) {
    // A well-formed node project, with a NESTED node_modules that must not be
    // reported separately (its bytes belong to the outer artifact).
    write_file(&root.join("node-proj/package.json"), b"{}");
    for i in 0..10 {
        write_file(
            &root.join(format!("node-proj/node_modules/pkg-a/f{i}.js")),
            &pseudo_random(1000, i as u64),
        );
    }
    write_file(
        &root.join("node-proj/node_modules/inner/node_modules/x.js"),
        b"nested",
    );

    // node_modules WITHOUT a package.json sibling: ignored.
    write_file(&root.join("node-stray/node_modules/x.js"), b"stray");

    // An EMPTY node_modules IS reported today (node has no zero-size skip).
    write_file(&root.join("node-empty/package.json"), b"{}");
    fs::create_dir_all(root.join("node-empty/node_modules")).unwrap();

    // Hard-link pair inside one artifact: allocated counted ONCE, apparent
    // counted twice (per-artifact inode dedup).
    write_file(&root.join("node-hl/package.json"), b"{}");
    let blob = pseudo_random(1 << 20, 42);
    write_file(&root.join("node-hl/node_modules/blob.bin"), &blob);
    fs::hard_link(
        root.join("node-hl/node_modules/blob.bin"),
        root.join("node-hl/node_modules/link.bin"),
    )
    .unwrap();

    // A sparse 2 GiB file: tiny allocated, huge apparent → apparent_bytes.
    write_file(&root.join("node-sparse/package.json"), b"{}");
    fs::create_dir_all(root.join("node-sparse/node_modules")).unwrap();
    let f = fs::File::create(root.join("node-sparse/node_modules/sparse.bin")).unwrap();
    f.set_len(2 << 30).unwrap();

    // A node_modules that is a SYMLINK (G-SCAN-03): never followed, never
    // reported — the walk must not descend into its target either.
    write_file(&root.join("node-symlink/package.json"), b"{}");
    std::os::unix::fs::symlink(
        root.join("node-proj/node_modules"),
        root.join("node-symlink/node_modules"),
    )
    .unwrap();

    // A rust project with a real target/.
    write_file(&root.join("rust-proj/Cargo.toml"), b"[package]\nname=\"p\"");
    write_file(
        &root.join("rust-proj/target/debug/app"),
        &pseudo_random(1000, 7),
    );

    // An EMPTY target/ is skipped (rust has a zero-size skip).
    write_file(
        &root.join("rust-empty/Cargo.toml"),
        b"[package]\nname=\"p\"",
    );
    fs::create_dir_all(root.join("rust-empty/target")).unwrap();

    // Cargo.toml without target/: nothing to report.
    write_file(
        &root.join("rust-no-target/Cargo.toml"),
        b"[package]\nname=\"p\"",
    );

    // target/ WITHOUT Cargo.toml: nothing to report.
    write_file(&root.join("rust-stray-target/target/app"), b"x");

    // All three python cache dirs in one project.
    write_file(
        &root.join("py-proj/__pycache__/mod.pyc"),
        &pseudo_random(1000, 8),
    );
    write_file(&root.join("py-proj/.pytest_cache/v/cache"), b"cache");
    write_file(&root.join("py-proj/.mypy_cache/v/cache"), b"cache");

    // An EMPTY __pycache__ is skipped (python has a zero-size skip).
    fs::create_dir_all(root.join("py-empty/__pycache__")).unwrap();

    // A venv is never reported AS a venv — but today's rule matches any
    // directory named __pycache__, wherever it sits, so the one inside the
    // venv IS reported. Characterized, not endorsed: the refactor must not
    // silently change it either way.
    write_file(&root.join("py-venv/.venv/pyvenv.cfg"), b"home = /usr");
    write_file(
        &root.join("py-venv/.venv/lib/__pycache__/x.pyc"),
        &pseudo_random(500, 9),
    );

    // Depth boundaries (WalkDir depth: root=0, direct children=1, so the
    // `bound/` prefix itself is depth 1 and eats one level of budget).
    // node_modules at depth 6: matched. At depth 7: invisible.
    write_file(&root.join("bound/a/b/c/d/package.json"), b"{}");
    write_file(&root.join("bound/a/b/c/d/node_modules/x.js"), b"x");
    write_file(&root.join("bound/f/g/h/i/j/package.json"), b"{}");
    write_file(&root.join("bound/f/g/h/i/j/node_modules/x.js"), b"x");

    // Cargo.toml at depth 5: matched. At depth 6: invisible.
    write_file(&root.join("bound/m/n/o/Cargo.toml"), b"[package]");
    write_file(&root.join("bound/m/n/o/target/app"), b"x");
    write_file(&root.join("bound/q/r/s/t/Cargo.toml"), b"[package]");
    write_file(&root.join("bound/q/r/s/t/target/app"), b"x");

    // __pycache__ at depth 6: matched. At depth 7: invisible.
    write_file(&root.join("bound/v/w/x/y/__pycache__/c.pyc"), b"c");
    write_file(&root.join("bound/aa/bb/cc/dd/ee/__pycache__/c.pyc"), b"c");

    // A symlink loop: with follow_links(false) the walk never enters it, so
    // it terminates; a valid artifact AFTER it in the walk still proves the
    // walk kept going.
    fs::create_dir_all(root.join("loop")).unwrap();
    std::os::unix::fs::symlink(".", root.join("loop/self")).unwrap();
    write_file(&root.join("loop/proj/package.json"), b"{}");
    write_file(&root.join("loop/proj/node_modules/x.js"), b"x");
}

// ------------------------------------------------------------------ driver

/// The part that changes across Wave C. Today: three independent walks.
/// After the refactor it drives the shared walk instead — the fixture and
/// the assertions below must not need a single edit.
fn scan_tree_providers(root: &Path) -> Vec<Resource> {
    let cancel = AtomicBool::new(false);
    let roots = [root.to_path_buf()];
    let mut out = Vec::new();
    out.extend(NodeModulesProvider.scan(&roots, &cancel));
    out.extend(RustTargetProvider.scan(&roots, &cancel));
    out.extend(PythonCacheProvider.scan(&roots, &cancel));
    out
}

// -------------------------------------------------------------- assertions

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

fn structural_set(root: &Path, found: &[Resource]) -> BTreeSet<(String, String)> {
    found
        .iter()
        .map(|r| (r.provider.clone(), rel(root, &r.path)))
        .collect()
}

fn expected_set() -> BTreeSet<(String, String)> {
    [
        ("node_modules", "node-proj/node_modules"),
        ("node_modules", "node-empty/node_modules"),
        ("node_modules", "node-hl/node_modules"),
        ("node_modules", "node-sparse/node_modules"),
        ("node_modules", "bound/a/b/c/d/node_modules"),
        ("node_modules", "loop/proj/node_modules"),
        ("rust_target", "rust-proj/target"),
        ("rust_target", "bound/m/n/o/target"),
        ("python_cache", "py-proj/__pycache__"),
        ("python_cache", "py-proj/.pytest_cache"),
        ("python_cache", "py-proj/.mypy_cache"),
        ("python_cache", "py-venv/.venv/lib/__pycache__"),
        ("python_cache", "bound/v/w/x/y/__pycache__"),
    ]
    .into_iter()
    .map(|(p, r)| (p.to_string(), r.to_string()))
    .collect()
}

fn find<'a>(found: &'a [Resource], provider: &str, relpath: &str, root: &Path) -> &'a Resource {
    found
        .iter()
        .find(|r| r.provider == provider && rel(root, &r.path) == relpath)
        .unwrap_or_else(|| {
            panic!(
                "missing {provider}:{relpath}; have {:?}",
                structural_set(root, found)
            )
        })
}

// ------------------------------------------------------------------ tests

#[test]
fn the_tree_rules_report_the_expected_resource_set() {
    let root = scratch("set");
    build_fixture(&root);
    let found = scan_tree_providers(&root);
    assert_eq!(
        structural_set(&root, &found),
        expected_set(),
        "the walk found a different set of artifacts"
    );
    for r in &found {
        assert_eq!(r.safety, devclean_core::SafetyLevel::Safe);
        assert!(
            r.blocked_reason.is_none(),
            "{} should carry no blocked_reason",
            r.path.display()
        );
        assert_eq!(r.reclaimable_bytes, r.size_bytes);
    }
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn measurement_semantics_are_preserved() {
    let root = scratch("measure");
    build_fixture(&root);
    let found = scan_tree_providers(&root);

    // Ordinary artifact: sized by content, no apparent annotation.
    let ordinary = find(&found, "node_modules", "node-proj/node_modules", &root);
    assert!(
        ordinary.size_bytes >= 10 * 1000,
        "expected ≥10 KiB of package files, got {}",
        ordinary.size_bytes
    );
    assert!(ordinary.apparent_bytes.is_none());

    // Hard links deduped WITHIN the artifact: 2 MiB of links, ~1 MiB counted.
    let hl = find(&found, "node_modules", "node-hl/node_modules", &root);
    assert!(
        hl.size_bytes < (2 << 20),
        "hard links must be counted once per artifact, got {}",
        hl.size_bytes
    );
    assert!(hl.size_bytes >= (1 << 20));
    assert!(
        hl.apparent_bytes.is_none(),
        "2 MiB apparent is not material"
    );

    // Sparse file: allocated tiny, apparent reported (material).
    let sparse = find(&found, "node_modules", "node-sparse/node_modules", &root);
    assert!(
        sparse.size_bytes < (64 << 20),
        "a sparse file must not report allocated ≈ its length, got {}",
        sparse.size_bytes
    );
    assert_eq!(sparse.apparent_bytes, Some(2 << 30));

    // Quirks: empty node_modules IS reported with zero; empty target and
    // empty __pycache__ are NOT reported (covered by the structural set —
    // rust-empty/, py-empty/ appear nowhere — and the zero here pins the
    // node side of that split).
    let empty = find(&found, "node_modules", "node-empty/node_modules", &root);
    assert_eq!(empty.size_bytes, 0);

    // Every artifact measures its own subtree: the nested node_modules'
    // bytes live inside the outer artifact only.
    let outer = find(&found, "node_modules", "node-proj/node_modules", &root);
    assert!(outer.size_bytes >= 10 * 1000 + 6);

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_unreadable_subtree_is_skipped_not_fatal() {
    // G-SCAN-06's other half: an entry the walk cannot read is skipped, and
    // the artifact still reports what it could measure. (Under root the
    // chmod is ineffective and the dir reads fine — the assertion holds
    // either way, which is why it needs no root guard.)
    let root = scratch("unreadable");
    write_file(&root.join("proj/package.json"), b"{}");
    write_file(&root.join("proj/node_modules/ok.js"), b"x");
    let locked = root.join("proj/node_modules/locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("secret.js"), b"y").unwrap();
    let mut perms = fs::metadata(&locked).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o000);
    fs::set_permissions(&locked, perms.clone()).unwrap();

    let found = scan_tree_providers(&root);
    let hit = find(&found, "node_modules", "proj/node_modules", &root);
    assert!(
        hit.size_bytes > 0,
        "the readable half must still be measured"
    );

    perms.set_mode(0o700);
    fs::set_permissions(&locked, perms).unwrap();
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn g_scan_03_a_symlinked_node_modules_is_never_reported_or_followed() {
    let root = scratch("symlink");
    build_fixture(&root);
    let found = scan_tree_providers(&root);
    // node-symlink/node_modules is a symlink to node-proj/node_modules: not
    // a resource of its own, and its target is not re-reported through it.
    assert!(
        !structural_set(&root, &found).contains(&(
            "node_modules".to_string(),
            "node-symlink/node_modules".to_string()
        )),
        "a symlink must never be reported as an artifact"
    );
    for r in &found {
        let p = rel(&root, &r.path);
        assert!(
            !p.starts_with("node-symlink/node_modules"),
            "the walk followed a symlink into {p}"
        );
    }
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn g_scan_03_a_symlink_loop_terminates_and_the_walk_continues() {
    let root = scratch("loop");
    build_fixture(&root);
    let found = scan_tree_providers(&root);
    // The artifact after the loop entry is still found: the walk did not
    // hang or abort at `loop/self -> .`.
    assert!(structural_set(&root, &found).contains(&(
        "node_modules".to_string(),
        "loop/proj/node_modules".to_string()
    )));
    fs::remove_dir_all(&root).unwrap();
}
