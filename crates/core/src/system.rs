//! Platform helpers. No assumption that cache locations are equal across
//! OSes — every path has a per-platform branch.

/// `"macos"`, `"windows"`, or `"linux"`.
pub fn platform_name() -> &'static str {
    std::env::consts::OS
}

/// `$HOME`, falling back to `USERPROFILE` on Windows.
pub fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from))
}

/// Folder names that commonly hold developer work, in the order we prefer
/// them.
///
/// This list is a *guess*, not a discovery mechanism —
/// [`detect_scan_roots`] is the discovery path, and Settings offers both. A
/// machine with projects in `~/work/oss` would otherwise need the path typed
/// by hand.
///
/// Note the case behaviour: macOS and Windows are case-insensitive by default,
/// so `Work` also matches an existing `~/work`. That is intended for a user's
/// own `~/work`, and it has one visible side effect worth knowing about —
/// GitHub's macOS runners create `/Users/runner/work` as their working
/// directory, so a scan there reports the runner's own build tree. Tests must
/// not depend on which folders this returns; see G-CI-03.
const DEV_FOLDER_NAMES: &[&str] = &[
    "Projects",
    "Code",
    "Workspace",
    "Dev",
    "Repos",
    "Src",
    "Work",
];

/// Default roots DevClean scans: well-known dev folders under `$HOME`,
/// filtered to directories that actually exist. Users can extend this in
/// Settings (or use `detect_scan_roots`); the safety validator always
/// re-checks containment.
pub fn default_scan_roots() -> Vec<std::path::PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    DEV_FOLDER_NAMES
        .iter()
        .map(|d| home.join(d))
        .filter(|p| p.is_dir())
        .collect()
}

/// A folder worth offering as a scan root, with enough evidence for the
/// user to judge it without opening it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ScanRootCandidate {
    pub path: std::path::PathBuf,
    /// Bytes held by recognised caches under this folder. `None` when the
    /// walk was skipped (depth cap) rather than measured.
    pub approximate_bytes: Option<u64>,
    /// How many `node_modules` / `target` / `__pycache__` dirs were found.
    pub cache_count: u32,
    /// True when the path is already in `Settings.scan_roots`.
    pub already_configured: bool,
}

/// Directory names that mark a folder as holding developer work. Used only
/// to RANK candidates, never as a safety input.
const CACHE_MARKERS: &[&str] = &["node_modules", "target", "__pycache__", ".venv"];

/// Discover candidate scan roots under `$HOME`, largest first.
///
/// Deliberately shallow (`max_depth` 2) and read-only: this powers a
/// "Detect folders" button, so it must be fast and must never mutate
/// anything. Hidden directories are skipped — `~/.config`, `~/.cache` and
/// friends hold caches the providers already own directly, and offering
/// them as roots would only slow every scan.
pub fn detect_scan_roots(configured: &[std::path::PathBuf]) -> Vec<ScanRootCandidate> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    detect_scan_roots_in(&home, configured)
}

/// [`detect_scan_roots`] against an explicit home directory. Taking the home
/// as an argument is what lets the tests run against a fixture instead of
/// whatever happens to be in the developer's `$HOME` (G-CI-03).
fn detect_scan_roots_in(
    home: &std::path::Path,
    configured: &[std::path::PathBuf],
) -> Vec<ScanRootCandidate> {
    let mut out = Vec::new();

    // Depth 1: the well-known names, always worth showing.
    for name in DEV_FOLDER_NAMES {
        let p = home.join(name);
        if p.is_dir() {
            out.push(candidate(p, configured));
        }
    }

    // Depth 2: anything that looks like a project collection.
    let Ok(entries) = std::fs::read_dir(home) else {
        return sort_candidates(out);
    };
    for e in entries.flatten() {
        let p = e.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with('.') || DEV_FOLDER_NAMES.contains(&name) {
            continue;
        }
        if !p.is_dir() {
            continue;
        }
        if p.join("package.json").is_file() || p.join("Cargo.toml").is_file() {
            out.push(candidate(p, configured));
        } else if let Ok(subs) = std::fs::read_dir(&p) {
            // A folder holding project manifests one level down.
            let looks_like_collection = subs.flatten().any(|s| {
                s.path().join("package.json").is_file() || s.path().join("Cargo.toml").is_file()
            });
            if looks_like_collection {
                out.push(candidate(p, configured));
            }
        }
    }
    sort_candidates(out)
}

fn candidate(path: std::path::PathBuf, configured: &[std::path::PathBuf]) -> ScanRootCandidate {
    let (bytes, count) = measure_markers(&path);
    ScanRootCandidate {
        already_configured: configured.contains(&path),
        path,
        approximate_bytes: Some(bytes),
        cache_count: count,
    }
}

/// Subdirectories sampled per marker when estimating its size. A
/// `node_modules` holds thousands of packages; reading all of them would
/// take seconds. Sampling a fixed number and scaling is accurate enough to
/// RANK candidates and never claims to be exact.
const SAMPLE_SUBDIRS: usize = 40;
/// How deep below a candidate root we look for marker folders.
const MARKER_DISCOVERY_DEPTH: usize = 3;

/// Sum an ESTIMATE of the bytes held by cache-marker folders under `root`,
/// and count the markers found.
///
/// Bounded twice over: marker discovery prunes *into* markers (so it never
/// descends into a `node_modules`), and each marker's size is sampled over
/// [`SAMPLE_SUBDIRS`] subdirectories. The result is explicitly an estimate —
/// `ScanRootCandidate.approximate_bytes`, never a promise.
fn measure_markers(root: &std::path::Path) -> (u64, u32) {
    let mut bytes = 0u64;
    let mut count = 0u32;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .max_depth(MARKER_DISCOVERY_DEPTH)
        .into_iter()
        // `filter_entry(false)` prunes the entry AND its subtree. We need
        // the opposite for markers: yield the marker, never its contents.
        // So the predicate only stops the walk at the depth limit and below
        // anything that already sits inside a marker — the marker itself is
        // let through and measured on its own.
        .filter_entry(|e| {
            e.depth() < MARKER_DISCOVERY_DEPTH && !has_marker_ancestor(e.path(), root)
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_dir() || !is_marker(entry.file_name().to_str()) {
            continue;
        }
        count += 1;
        bytes = bytes.saturating_add(estimate_marker_bytes(entry.path()));
    }
    (bytes, count)
}

/// True when any strict ancestor of `path` (under `root`) is a cache
/// marker — i.e. `path` is content inside a marker we already measured.
fn has_marker_ancestor(path: &std::path::Path, root: &std::path::Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let comps: Vec<_> = rel.components().collect();
    comps[..comps.len().saturating_sub(1)]
        .iter()
        .any(|c| is_marker(c.as_os_str().to_str()))
}

fn is_marker(name: Option<&str>) -> bool {
    name.is_some_and(|n| CACHE_MARKERS.contains(&n))
}

/// Sampled size estimate for one marker folder: direct files counted
/// exactly, subdirectories sampled and scaled by their real count.
fn estimate_marker_bytes(marker: &std::path::Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(marker) else {
        return 0;
    };
    let mut direct = 0u64;
    let mut subs: Vec<std::path::PathBuf> = Vec::new();
    for e in rd.flatten() {
        match e.file_type() {
            Ok(ft) if ft.is_file() => direct += e.metadata().map(|m| m.len()).unwrap_or(0),
            Ok(ft) if ft.is_dir() => subs.push(e.path()),
            _ => {}
        }
    }
    if subs.is_empty() {
        return direct;
    }
    let n = subs.len().min(SAMPLE_SUBDIRS);
    let mut sample = 0u64;
    for p in subs.iter().take(n) {
        sample = sample.saturating_add(sample_bytes(p, SAMPLE_DEPTH));
    }
    direct + (sample / n as u64) * subs.len() as u64
}

/// Levels sampled inside a sampled subdirectory. Package contents usually
/// sit one level down (`pkg/dist/index.js`), so a single level badly
/// under-reports.
const SAMPLE_DEPTH: usize = 2;

/// Bounded recursive size estimate: at most [`SAMPLE_DEPTH`] levels deep.
fn sample_bytes(dir: &std::path::Path, depth: usize) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .filter_map(|e| {
            let ft = e.file_type().ok()?;
            if ft.is_file() {
                Some(e.metadata().map(|m| m.len()).unwrap_or(0))
            } else if ft.is_dir() && depth > 1 {
                Some(sample_bytes(&e.path(), depth - 1))
            } else {
                None
            }
        })
        .sum()
}

fn sort_candidates(mut v: Vec<ScanRootCandidate>) -> Vec<ScanRootCandidate> {
    v.sort_by(|a, b| {
        b.approximate_bytes
            .unwrap_or(0)
            .cmp(&a.approximate_bytes.unwrap_or(0))
            .then_with(|| a.path.cmp(&b.path))
    });
    v.dedup_by(|a, b| a.path == b.path);
    v
}

/// Directory for DevClean's own files (settings.json, scan cache, cleanup
/// history). Per-platform, never inside a scan root. Created on demand.
pub fn app_data_dir() -> std::path::PathBuf {
    let home = home_dir();
    let base = match std::env::consts::OS {
        "macos" => home.map(|h| h.join("Library/Application Support")),
        "windows" => std::env::var_os("APPDATA").map(std::path::PathBuf::from),
        _ => std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| home.map(|h| h.join(".config"))),
    };
    base.unwrap_or_else(std::env::temp_dir).join("devclean")
}

/// Directories that hold the tools the providers shell out to and that a
/// GUI app launched from Finder never inherits. launchd starts the process
/// with `/usr/bin:/bin:/usr/sbin:/sbin`, while `docker` lives in
/// `/usr/local/bin`, `go` under Homebrew's `/opt/homebrew/bin`, and so on.
/// Every CLI-backed provider treats a missing tool as "nothing to report"
/// (I-absence-is-normal), so without this they all silently returned empty
/// and Settings could only guess "the tool may not be installed".
///
/// Fixed server-side strings only — never anything from the frontend
/// (G-SAFE-01) — and no shell is executed to read them.
const PATH_CANDIDATES: &[&str] = &[
    "/usr/local/bin",
    "/usr/local/sbin",
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/opt/local/bin",
    "/opt/local/sbin",
];

/// Home-relative counterparts of [`PATH_CANDIDATES`].
const HOME_PATH_CANDIDATES: &[&str] = &[".local/bin", ".cargo/bin", ".bun/bin", ".volta/bin"];

/// Every directory macOS's own path files declare: `/etc/paths` plus each
/// entry in `/etc/paths.d`. That is the system's answer to "what a process
/// should see"; a Finder-launched app simply never gets asked.
fn system_path_dirs() -> Vec<std::path::PathBuf> {
    let mut files = vec![std::path::PathBuf::from("/etc/paths")];
    if let Ok(entries) = std::fs::read_dir("/etc/paths.d") {
        for e in entries.flatten() {
            files.push(e.path());
        }
    }
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() {
                out.push(std::path::PathBuf::from(line));
            }
        }
    }
    out
}

/// `v22.16.0` → `[22, 16, 0]`, so nvm's directory names compare as
/// versions instead of as strings — `v9.11.2` must not beat `v10.0.0`.
fn version_key(name: &str) -> Vec<u64> {
    name.trim_start_matches('v')
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

/// The `bin` of the highest installed nvm Node. nvm keeps one directory
/// per version and maintains no "current" symlink, so the choice is made
/// by version order here: appending every version would let an arbitrary
/// old `node` win PATH resolution.
fn highest_nvm_bin(versions: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(versions).ok()?;
    let mut best: Option<(Vec<u64>, std::path::PathBuf)> = None;
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let first = name.as_bytes().first();
        if !first.is_some_and(|b| b.is_ascii_digit() || *b == b'v') {
            continue;
        }
        let bin = p.join("bin");
        if !bin.is_dir() {
            continue;
        }
        let key = version_key(name);
        if best.as_ref().is_none_or(|(k, _)| &key > k) {
            best = Some((key, bin));
        }
    }
    best.map(|(_, bin)| bin)
}

/// `existing` plus every candidate that is not already listed, in
/// candidate order. Pure: the caller decides which candidates exist, so
/// the test never depends on the machine's own PATH (G-CI-03).
pub fn merge_path(
    existing: &[std::path::PathBuf],
    candidates: &[std::path::PathBuf],
) -> Vec<std::path::PathBuf> {
    let mut out = existing.to_vec();
    for c in candidates {
        if !out.contains(c) {
            out.push(c.clone());
        }
    }
    out
}

/// Every directory worth appending to this process's `PATH`, already
/// deduplicated. Existence is NOT checked here — [`augment_process_path`]
/// filters, which keeps this half testable against a fixture.
pub fn path_candidates() -> Vec<std::path::PathBuf> {
    let mut out = system_path_dirs();
    for d in PATH_CANDIDATES {
        out.push(std::path::PathBuf::from(d));
    }
    if let Some(home) = home_dir() {
        for d in HOME_PATH_CANDIDATES {
            out.push(home.join(d));
        }
        if let Some(bin) = highest_nvm_bin(&home.join(".nvm/versions/node")) {
            out.push(bin);
        }
    }
    merge_path(&[], &out)
}

/// Widen this process's `PATH` with [`path_candidates`] and return only
/// the directories that were actually added.
///
/// Called once, as the first statement of the desktop app's `run()`,
/// before the builder can spawn anything: a Finder-launched app inherits
/// launchd's minimal PATH, so `docker`, `go` and friends were invisible to
/// every provider and each reported "nothing found" without a word of
/// explanation. Nothing here executes a shell or reads anything the
/// frontend supplied (G-SAFE-01); directories that do not exist are
/// skipped, so absence stays absence (I-absence-is-normal).
pub fn augment_process_path() -> Vec<std::path::PathBuf> {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let existing: Vec<std::path::PathBuf> = std::env::split_paths(&current)
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    let candidates: Vec<std::path::PathBuf> = path_candidates()
        .into_iter()
        .filter(|c| c.is_dir())
        .collect();
    let merged = merge_path(&existing, &candidates);
    let added = merged[existing.len()..].to_vec();
    if added.is_empty() {
        return added;
    }
    // An entry containing `:` cannot be represented; skipping the update
    // keeps the current PATH rather than writing a corrupted one.
    if let Ok(joined) = std::env::join_paths(&merged) {
        std::env::set_var("PATH", joined);
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn app_data_dir_is_namespaced() {
        assert_eq!(
            app_data_dir().file_name().and_then(|s| s.to_str()),
            Some("devclean")
        );
    }

    fn home() -> PathBuf {
        home_dir().expect("test needs HOME set")
    }

    /// Regression: the old list was `Projects, Code, Workspace` only. On a
    /// machine whose work lives in a dev folder outside those three, that
    /// produced an EMPTY root list, so every root-driven provider
    /// (node_modules, rust_target, python_cache) found nothing at all.
    #[test]
    fn default_roots_cover_the_common_dev_folder_names() {
        let roots = default_scan_roots();
        for name in DEV_FOLDER_NAMES {
            let candidate = home().join(name);
            if candidate.is_dir() {
                assert!(
                    roots.contains(&candidate),
                    "{name} exists but is not a default scan root"
                );
            }
        }
    }

    #[test]
    fn default_roots_are_unique_existing_directories_under_home() {
        let roots = default_scan_roots();
        let mut seen = std::collections::HashSet::new();
        for r in &roots {
            assert!(r.is_dir(), "{} is not a directory", r.display());
            assert!(r.starts_with(home()), "{} escaped HOME", r.display());
            assert!(seen.insert(r.clone()), "{} listed twice", r.display());
        }
    }

    #[test]
    fn detect_returns_only_real_directories_never_hidden_ones() {
        let found = detect_scan_roots(&[]);
        for c in &found {
            assert!(c.path.is_dir(), "{} is not a directory", c.path.display());
            let name = c.path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            assert!(!name.starts_with('.'), "hidden dir offered: {name}");
        }
    }

    /// Runs against a fixture home, not the machine's own. The earlier version
    /// looked for `~/VOLANTIS` and returned early when it was absent, so on CI
    /// it asserted nothing at all (G-CI-03).
    #[test]
    fn detect_marks_already_configured_roots() {
        let home = std::env::temp_dir().join(format!("devclean-configured-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let project = home.join("Projects").join("web");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("package.json"), "{}").unwrap();

        let configured = home.join("Projects");
        let found = detect_scan_roots_in(&home, std::slice::from_ref(&configured));
        let c = found
            .iter()
            .find(|c| c.path == configured)
            .expect("~/Projects must be offered");
        assert!(c.already_configured);

        // …and the same path is NOT marked when it is not configured.
        let found = detect_scan_roots_in(&home, &[]);
        let c = found.iter().find(|c| c.path == configured).unwrap();
        assert!(!c.already_configured);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn detect_is_sorted_by_size_and_deduplicated() {
        let found = detect_scan_roots(&[]);
        let sizes: Vec<u64> = found
            .iter()
            .map(|c| c.approximate_bytes.unwrap_or(0))
            .collect();
        assert!(
            sizes.windows(2).all(|w| w[0] >= w[1]),
            "candidates must be largest-first: {sizes:?}"
        );
        let mut paths: Vec<_> = found.iter().map(|c| c.path.clone()).collect();
        let before = paths.len();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), before, "detect returned duplicates");
    }

    #[test]
    fn measure_markers_counts_nested_caches_and_ignores_source() {
        let root = std::env::temp_dir().join(format!("devclean-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/node_modules/pkg")).unwrap();
        std::fs::write(root.join("a/node_modules/pkg/index.js"), "x".repeat(500)).unwrap();
        std::fs::create_dir_all(root.join("b/target/debug")).unwrap();
        std::fs::write(root.join("b/target/debug/app"), "y".repeat(300)).unwrap();
        // Source bytes must never be counted.
        std::fs::create_dir_all(root.join("c/src")).unwrap();
        std::fs::write(root.join("c/src/main.rs"), "z".repeat(9_000)).unwrap();

        let (bytes, count) = measure_markers(&root);
        assert_eq!(count, 2, "expected node_modules + target, got {count}");
        assert_eq!(bytes, 800, "only cache bytes, not source bytes");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_estimate_scales_subdirectory_count() {
        // 100 sibling dirs of 10 B each = 1000 B, sampled over 3 → same
        // answer. The estimator must not just read the sample.
        let root = std::env::temp_dir().join(format!("devclean-scale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let nm = root.join("node_modules");
        std::fs::create_dir_all(&nm).unwrap();
        for i in 0..100 {
            let d = nm.join(format!("p{i}"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("index.js"), "x".repeat(10)).unwrap();
        }
        let (bytes, count) = measure_markers(&root);
        assert_eq!(count, 1);
        assert_eq!(
            bytes, 1_000,
            "sampled estimate must scale to the real total"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn discovery_does_not_descend_into_markers() {
        // A cache dir nested INSIDE a node_modules must not be counted
        // twice; pruning keeps discovery shallow and cheap.
        let root = std::env::temp_dir().join(format!("devclean-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("proj/node_modules/dep/target")).unwrap();
        std::fs::write(
            root.join("proj/node_modules/dep/target/app"),
            "x".repeat(400),
        )
        .unwrap();
        let (_, count) = measure_markers(&root);
        assert_eq!(count, 1, "only the outer node_modules counts");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_path_keeps_order_and_never_duplicates() {
        let existing = vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")];
        let candidates = vec![PathBuf::from("/bin"), PathBuf::from("/opt/homebrew/bin")];
        assert_eq!(
            merge_path(&existing, &candidates),
            vec![
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
                PathBuf::from("/opt/homebrew/bin"),
            ],
            "existing entries stay put, missing ones append in candidate order"
        );
    }

    #[test]
    fn merge_path_from_an_empty_path_is_exactly_the_candidates() {
        let candidates = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        assert_eq!(merge_path(&[], &candidates), candidates);
    }

    #[test]
    fn merge_path_is_idempotent() {
        let existing = vec![PathBuf::from("/usr/bin")];
        let candidates = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let once = merge_path(&existing, &candidates);
        assert_eq!(merge_path(&once, &candidates), once);
    }

    #[test]
    fn path_candidates_never_list_a_directory_twice() {
        let candidates = path_candidates();
        let mut sorted = candidates.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            candidates.len(),
            "duplicate in {candidates:?}"
        );
    }

    #[test]
    fn nvm_versions_compare_as_versions_not_as_strings() {
        assert!(
            version_key("v22.16.0") > version_key("v10.0.0"),
            "string order would put v10 first"
        );
        assert!(version_key("v9.11.2") < version_key("v10.0.0"));
        assert_eq!(version_key("v22.16.0"), vec![22, 16, 0]);
    }

    /// The nvm pick is decided by a fixture, never by whatever is installed
    /// on the machine (G-CI-03).
    #[test]
    fn highest_nvm_bin_picks_the_newest_installed_version() {
        let root = std::env::temp_dir().join(format!("devclean-nvm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for v in ["v10.0.0", "v22.16.0", "v9.11.2"] {
            std::fs::create_dir_all(root.join(v).join("bin")).unwrap();
            std::fs::write(root.join(v).join("bin").join("node"), "x").unwrap();
        }
        // A directory without `bin/` is not an installation.
        std::fs::create_dir_all(root.join("downloads")).unwrap();

        assert_eq!(
            highest_nvm_bin(&root),
            Some(root.join("v22.16.0/bin")),
            "the highest installed version must win, not the alphabetically last"
        );
        assert_eq!(
            highest_nvm_bin(&root.join("absent")),
            None,
            "no nvm install is not an error"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Self-consistency only: whatever this machine holds, the call may
    /// only ADD directories that exist, and a second call must add nothing.
    #[test]
    fn augment_only_adds_existing_directories_and_is_idempotent() {
        let added = augment_process_path();
        for d in &added {
            assert!(d.is_dir(), "{} was added but does not exist", d.display());
        }
        let now = std::env::var_os("PATH").unwrap_or_default();
        let now: Vec<PathBuf> = std::env::split_paths(&now).collect();
        for d in &added {
            assert!(
                now.contains(d),
                "{} missing from PATH after the call",
                d.display()
            );
        }
        assert!(
            augment_process_path().is_empty(),
            "a second call must be a no-op"
        );
    }
}
