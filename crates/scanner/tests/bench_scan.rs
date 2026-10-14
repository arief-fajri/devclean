//! Concurrency benchmark: is `DEFAULT_SCAN_CONCURRENCY = 4` actually the right
//! number on this machine?
//!
//! A scan is I/O bound, so "more workers = faster" is not a safe assumption:
//! 8 walkers on a spinning disk can be *slower* than 4, and page cache grows
//! with the fan-out. This exists so the default is chosen from a measurement
//! rather than from a feeling.
//!
//! Ignored by default — it builds a synthetic tree, so it is not something CI
//! should run. Run it explicitly:
//!
//! ```text
//! cargo test -p devclean-scanner --test bench_scan -- --ignored --nocapture
//! ```
//!
//! Point it at real work to get a number that matches your machine:
//!
//! ```text
//! DEVSCAN_BENCH_ROOT=~/Projects cargo test -p devclean-scanner --test bench_scan -- --ignored --nocapture
//! ```
//!
//! Read the output as: *time* is what the user waits, *peak* is what the scan
//! costs. Only change the default if time improves **and** peak stays sane.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use devclean_core::PROVIDER_SCAN_TIMEOUT;
use devclean_providers::all_providers;
use devclean_scanner::ScanManager;

/// Build a tree with the shape that makes scanning interesting: many project
/// directories, each holding a few nested cache directories with files.
fn build_synthetic_tree(root: &Path) -> std::io::Result<usize> {
    let mut dirs = 0usize;
    for project in 0..60 {
        let project_dir = root.join(format!("project-{project:03}"));
        for cache in ["node_modules", ".cache", "dist"] {
            let cache_dir = project_dir.join(cache);
            std::fs::create_dir_all(&cache_dir)?;
            dirs += 1;
            for file in 0..20 {
                std::fs::write(cache_dir.join(format!("f{file}.bin")), vec![0u8; 4096])?;
            }
        }
    }
    Ok(dirs)
}

fn run_once(root: &Path, concurrency: usize) -> Duration {
    let manager =
        ScanManager::with_concurrency(all_providers(), PROVIDER_SCAN_TIMEOUT, concurrency);
    let roots = vec![root.to_path_buf()];
    let started = Instant::now();
    let summary = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            manager
                .scan_all(
                    roots,
                    &[],
                    &[],
                    devclean_core::PolicyInputs::default(),
                    None,
                    None,
                )
                .await
        });
    let elapsed = started.elapsed();
    // Touch the result so the optimiser cannot skip the work.
    println!(
        "      found {} resources across {} providers",
        summary.resources.len(),
        summary.providers.len()
    );
    elapsed
}

#[test]
#[ignore = "benchmark: builds a synthetic tree and takes a while"]
fn scan_concurrency_four_vs_eight() {
    let explicit = std::env::var("DEVSCAN_BENCH_ROOT").ok().map(PathBuf::from);
    let using_real_dir = explicit.is_some();
    let root = match &explicit {
        Some(p) => {
            println!("scanning real directory: {}", p.display());
            p.clone()
        }
        None => {
            let p = std::env::temp_dir().join(format!("devclean-bench-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            let dirs = build_synthetic_tree(&p).expect("build tree");
            println!(
                "built synthetic tree: {} cache dirs under {}",
                dirs,
                p.display()
            );
            p
        }
    };
    println!("\nconcurrency   run 1      run 2      run 3     median");
    println!("-------------------------------------------------------");
    for concurrency in [4usize, 8] {
        let mut runs = Vec::new();
        for _ in 0..3 {
            runs.push(run_once(&root, concurrency));
        }
        runs.sort();
        println!(
            "{:>11}   {:>8.3}s  {:>8.3}s  {:>8.3}s  {:>8.3}s",
            concurrency,
            runs[0].as_secs_f64(),
            runs[1].as_secs_f64(),
            runs[2].as_secs_f64(),
            runs[1].as_secs_f64()
        );
    }

    if !using_real_dir {
        let _ = std::fs::remove_dir_all(&root);
    }
    println!(
        "\nIf 8 is not clearly faster (and the machine is not spinning rust),\n\
         leave DEFAULT_SCAN_CONCURRENCY alone."
    );
}
