//! Wave C benchmark: the shared project-tree walk against the shape it
//! replaced.
//!
//! `bench_scan` measures a whole `ScanManager` run, where the fixed-path
//! caches under `$HOME` dominate and the tree walk is a rounding error. This
//! bench isolates the tree rules so the Wave C change is measurable on its
//! own. It compares three costs over the same synthetic dev tree:
//!
//! * `new` — `walk_tree_rules` over all three rules: one traversal, sizing
//!   folded into the same pass.
//! * `old_walks` — the three rules walked one at a time (what the old
//!   providers did: three traversals of the same tree).
//! * `old_sizing` — the shared walk PLUS a `dir_size_detail` re-walk of
//!   every row (what the old providers did per match).
//!
//! Same matching code, same sizing primitive; only "shared vs repeated" and
//! "folded vs re-walked" differ, so the ratio is the honest Wave C effect.
//!
//! Ignored by default — it builds a synthetic tree. Run it explicitly:
//!
//! ```text
//! cargo test -p devclean-scanner --test bench_tree_walk -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use devclean_providers::{
    dir_size_detail, walk_tree_rules, NodeModulesProvider, PythonCacheProvider, RustTargetProvider,
    TreeRule,
};

/// A dev tree shaped so every rule matches, and the traversal is not
/// trivial: 40 node projects, each with a `target`-free `.cache`/`dist`
/// noise pair, plus rust and python projects.
fn build_tree(root: &Path) -> std::io::Result<usize> {
    let mut artifacts = 0usize;
    for project in 0..40 {
        let p = root.join(format!("node-{project:03}"));
        std::fs::create_dir_all(p.join("node_modules"))?;
        std::fs::write(p.join("package.json"), b"{}")?;
        for file in 0..30 {
            std::fs::write(p.join(format!("node_modules/f{file}.js")), vec![0u8; 2048])?;
        }
        // Non-matching noise the walk still has to step over.
        std::fs::create_dir_all(p.join(".cache/deep/deeper"))?;
        std::fs::write(p.join(".cache/deep/deeper/x"), vec![0u8; 2048])?;
        std::fs::create_dir_all(p.join("dist"))?;
        std::fs::write(p.join("dist/bundle.js"), vec![0u8; 2048])?;
        artifacts += 1;
    }
    for project in 0..20 {
        let p = root.join(format!("rust-{project:03}"));
        std::fs::create_dir_all(p.join("target/debug"))?;
        std::fs::write(p.join("Cargo.toml"), b"[package]")?;
        for file in 0..20 {
            std::fs::write(p.join(format!("target/debug/f{file}.o")), vec![0u8; 2048])?;
        }
        artifacts += 1;
    }
    for project in 0..20 {
        let p = root.join(format!("py-{project:03}"));
        std::fs::create_dir_all(p.join("__pycache__"))?;
        for file in 0..10 {
            std::fs::write(p.join(format!("__pycache__/f{file}.pyc")), vec![0u8; 1024])?;
        }
        artifacts += 1;
    }
    Ok(artifacts)
}

fn rules() -> Vec<Arc<dyn TreeRule>> {
    vec![
        Arc::new(NodeModulesProvider),
        Arc::new(RustTargetProvider),
        Arc::new(PythonCacheProvider),
    ]
}

/// New: one traversal, sizing folded in.
fn new_shared(root: &Path) -> usize {
    let cancel = AtomicBool::new(false);
    let groups = walk_tree_rules(std::slice::from_ref(&root.to_path_buf()), &rules(), &cancel);
    groups.iter().map(Vec::len).sum()
}

/// Old traversal shape: one full walk per rule.
fn old_walks(root: &Path) -> usize {
    let cancel = AtomicBool::new(false);
    let roots = [root.to_path_buf()];
    let mut n = 0;
    for rule in rules() {
        let groups = walk_tree_rules(&roots, std::slice::from_ref(&rule), &cancel);
        n += groups.iter().map(Vec::len).sum::<usize>();
    }
    n
}

/// Old sizing shape: shared walk, then re-measure every row.
fn old_sizing(root: &Path) -> usize {
    let cancel = AtomicBool::new(false);
    let groups = walk_tree_rules(std::slice::from_ref(&root.to_path_buf()), &rules(), &cancel);
    let mut n = 0;
    for group in &groups {
        for r in group {
            std::hint::black_box(dir_size_detail(&r.path));
            n += 1;
        }
    }
    n
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn time(mut f: impl FnMut() -> usize) -> Duration {
    let t = Instant::now();
    let n = std::hint::black_box(f());
    let elapsed = t.elapsed();
    println!("      rows: {n}");
    elapsed
}

#[test]
#[ignore = "benchmark: builds a synthetic tree and takes a while"]
fn shared_walk_vs_three_walks_and_re_measurement() {
    let root = std::env::temp_dir().join(format!("devclean-bench-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let artifacts = build_tree(&root).expect("build tree");
    println!(
        "built synthetic tree: {artifacts} artifacts under {}",
        root.display()
    );

    println!("\napproach          run 1      run 2      run 3     median");
    println!("--------------------------------------------------------");
    type Case = (&'static str, fn(&Path) -> usize);
    let cases: [Case; 3] = [
        ("new shared", new_shared),
        ("old 3 walks", old_walks),
        ("old + re-measure", old_sizing),
    ];
    let mut medians = Vec::new();
    for (name, f) in cases {
        let mut runs = Vec::new();
        for _ in 0..3 {
            runs.push(time(|| f(&root)));
        }
        let m = median(runs.clone());
        medians.push((name, m));
        runs.sort();
        println!(
            "{:>16}   {:>8.3}s  {:>8.3}s  {:>8.3}s  {:>8.3}s",
            name,
            runs[0].as_secs_f64(),
            runs[1].as_secs_f64(),
            runs[2].as_secs_f64(),
            m.as_secs_f64()
        );
    }

    let _ = std::fs::remove_dir_all(&root);
    if let [(_, new), (_, walks), (_, sizing)] = medians.as_slice() {
        println!(
            "\nvs three walks: {:.2}x   vs three walks + re-measure: {:.2}x",
            walks.as_secs_f64() / new.as_secs_f64(),
            sizing.as_secs_f64() / new.as_secs_f64()
        );
    }
}
