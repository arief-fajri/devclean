//! Age-based pruning — the retention primitive (I9).
//!
//! `RemoveDir` is all-or-nothing: cleaning a cache wipes it. For anything
//! with per-item history (AI session logs, conversation transcripts) that is
//! the wrong default, so `CleanupAction::PruneOlderThan` removes only the
//! entries whose mtime is older than the cutoff and leaves the rest alone.
//!
//! Two rules are absolute:
//!
//! 1. The cutoff comes from `Settings.retention_days`, which defaults to
//!    `None`. With no policy configured this function is never reached —
//!    the resource carries no `blocked_reason` and is deleted outright.
//! 2. Nothing is ever deleted before the cutoff. There is no "clean if
//!    bigger than X" shortcut: age is the only criterion.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Bytes a prune would remove from `dir` right now, without removing
/// anything. Drives the preview so the dialog shows the real number.
pub fn reclaimable_bytes(dir: &Path, min_age_days: u32, pattern: &str, skip: &[&str]) -> u64 {
    let Some(cutoff) = cutoff(min_age_days) else {
        return 0;
    };
    entries(dir, pattern, skip)
        .into_iter()
        .filter(|p| is_older(p, cutoff))
        .map(|p| size_of(&p))
        .sum()
}

/// Remove entries under `dir` older than `min_age_days`. Returns the number
/// of bytes actually removed — measured after the fact, never assumed.
pub fn prune(
    dir: &Path,
    min_age_days: u32,
    pattern: &str,
    prune_empty_dirs: bool,
    skip: &[&str],
) -> u64 {
    let Some(cutoff) = cutoff(min_age_days) else {
        return 0;
    };
    let mut freed = 0u64;
    for path in entries(dir, pattern, skip) {
        if !is_older(&path, cutoff) {
            continue;
        }
        let before = size_of(&path);
        let ok = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if ok.is_ok() {
            freed = freed.saturating_add(before);
        }
    }
    if prune_empty_dirs {
        prune_empties(dir);
    }
    freed
}

/// Remove directories under `root` that hold nothing. Depth-first, and
/// skipped when `root` itself would end up empty — the caller owns `root`
/// and providers keep it on purpose.
fn prune_empties(root: &Path) {
    let mut dirs: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.path().to_path_buf())
        .collect();
    // Deepest first, so a directory emptied by this pass is seen as empty.
    dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for d in dirs {
        if is_effectively_empty(&d) {
            let _ = std::fs::remove_dir(&d);
        }
    }
}

fn is_effectively_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut rd| rd.next().is_none())
        .unwrap_or(false)
}

fn entries(dir: &Path, pattern: &str, skip: &[&str]) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| {
            e.file_type()
                .map(|t| t.is_dir() || t.is_file())
                .unwrap_or(false)
        })
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // I10: the skip list wins over the pattern, always.
            !skip.contains(&name.as_str()) && matches_glob(&name, pattern)
        })
        .map(|e| e.path())
        .collect()
}

/// Minimal glob: supports `*` (any run within one path segment) and `?`.
/// Deliberately not a regex — the pattern is always a provider constant,
/// and a full matcher would be a needless attack surface (G-SAFE-02).
fn matches_glob(name: &str, pattern: &str) -> bool {
    // A pattern matches ONE path segment. Without this, `*` would happily
    // match across separators and a caller could reach deeper than the
    // provider intended.
    if name.contains('/') || name.contains('\\') {
        return false;
    }
    if pattern == "*" {
        return true;
    }
    let (n, p): (Vec<char>, Vec<char>) = (name.chars().collect(), pattern.chars().collect());
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while i < n.len() {
        if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '*' {
            star = j;
            j += 1;
            mark = i;
        } else if star != usize::MAX {
            j = star + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == '*' {
        j += 1;
    }
    j == p.len()
}

fn cutoff(min_age_days: u32) -> Option<SystemTime> {
    SystemTime::now().checked_sub(Duration::from_secs(u64::from(min_age_days) * 86_400))
}

/// Age test based on mtime. A path with an unreadable mtime is treated as
/// RECENT, so it is never pruned: when in doubt, keep the data.
fn is_older(path: &Path, cutoff: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|m| m < cutoff)
        .unwrap_or(false)
}

fn size_of(path: &Path) -> u64 {
    if path.is_dir() {
        walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    } else {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("devclean-ret-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn aged_file(dir: &Path, name: &str, days: u64, bytes: usize) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, "x".repeat(bytes)).unwrap();
        let t = SystemTime::now() - Duration::from_secs(days * 86_400);
        filetime::set_mtime(&p, t);
        p
    }

    /// `std::fs::File::set_modified` (Rust 1.75+) sets mtime without pulling
    /// in a dependency or shelling out to `touch`.
    mod filetime {
        use std::fs::File;
        use std::path::Path;
        use std::time::SystemTime;

        pub fn set_mtime(p: &Path, t: SystemTime) {
            if let Ok(f) = File::options().write(true).open(p) {
                let _ = f.set_modified(t);
            }
        }
    }

    #[test]
    fn prune_removes_only_old_entries() {
        let d = root("old-only");
        aged_file(&d, "old.log", 90, 1000);
        aged_file(&d, "fresh.log", 2, 1000);
        // 31 days is PAST a 30-day retention, so it goes too.
        aged_file(&d, "edge.log", 31, 500);

        let freed = prune(&d, 30, "*.log", false, &[]);
        assert_eq!(freed, 1500, "the 90d and 31d files go");
        assert!(!d.join("old.log").exists());
        assert!(!d.join("edge.log").exists());
        assert!(d.join("fresh.log").exists(), "2 days old must survive");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn reclaimable_bytes_matches_what_prune_actually_removes() {
        let d = root("measure");
        aged_file(&d, "a.bin", 90, 2000);
        aged_file(&d, "b.bin", 90, 1000);
        aged_file(&d, "c.bin", 1, 9999);

        let predicted = reclaimable_bytes(&d, 30, "*.bin", &[]);
        let actual = prune(&d, 30, "*.bin", false, &[]);
        assert_eq!(
            predicted, actual,
            "the preview number must equal the cleanup result"
        );
        assert_eq!(actual, 3000);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn pattern_filters_what_is_touched() {
        let d = root("pattern");
        aged_file(&d, "keep.md", 90, 1000);
        aged_file(&d, "drop.log", 90, 1000);
        prune(&d, 30, "*.log", false, &[]);
        assert!(!d.join("drop.log").exists());
        assert!(
            d.join("keep.md").exists(),
            "a different extension must be untouched"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_zero_day_cutoff_prunes_everything_eligible() {
        let d = root("zero");
        aged_file(&d, "x", 0, 10);
        let freed = prune(&d, 0, "*", false, &[]);
        assert!(freed >= 10, "everything passes a 0-day cutoff");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn glob_matching_is_segment_local_and_anchored() {
        assert!(matches_glob("anything", "*"));
        assert!(matches_glob("a.log", "*.log"));
        assert!(matches_glob("a.log", "a.lo?"));
        assert!(!matches_glob("a.txt", "*.log"));
        assert!(!matches_glob("deep/a.log", "*.log"), "no path separator");
        assert!(matches_glob("abc", "a*c"));
        // `*` is "zero or more", per glob convention.
        assert!(matches_glob("ac", "a*c"));
        assert!(!matches_glob("bc", "a*c"));
        assert!(matches_glob("anything.at.all", "*.at.*"));
    }

    #[test]
    fn prune_empty_dirs_removes_only_those_left_empty() {
        let d = root("empties");
        let old_empty = d.join("old");
        fs::create_dir_all(&old_empty).unwrap();
        let old_full = d.join("full");
        fs::create_dir_all(&old_full).unwrap();
        aged_file(&old_full, "keep.txt", 90, 10);

        prune(&d, 30, "*", true, &[]);
        assert!(!old_empty.exists(), "an empty dir should be removed");
        assert!(old_full.exists(), "a dir with content must stay");
        assert!(d.exists(), "the root itself is never removed");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn an_unreadable_mtime_keeps_the_file() {
        let d = root("unreadable");
        let p = d.join("weird");
        fs::write(&p, "x").unwrap();
        // A path that does not exist is "recent" by construction.
        let cutoff = SystemTime::now();
        assert!(!is_older(&d.join("does-not-exist"), cutoff));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_skip_list_beats_a_broad_pattern() {
        // I10: `*` would match everything, but the skip list must still
        // leave settings and credentials alone — at any depth.
        let d = root("skiplist");
        aged_file(&d, "session.jsonl", 90, 1000);
        aged_file(&d, "settings.json", 90, 500);
        aged_file(&d, "auth.json", 90, 300);

        let freed = prune(&d, 30, "*", false, &["settings.json", "auth.json"]);
        assert_eq!(freed, 1000, "only the session goes");
        assert!(!d.join("session.jsonl").exists());
        assert!(d.join("settings.json").exists(), "settings survives");
        assert!(d.join("auth.json").exists(), "credentials survive");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_estimate_respects_the_skip_list_too() {
        // If the preview ignored `skip`, the dialog would promise more
        // than the cleanup frees.
        let d = root("skip-estimate");
        aged_file(&d, "keep.json", 90, 400);
        aged_file(&d, "drop.json", 90, 600);
        let predicted = reclaimable_bytes(&d, 30, "*", &["keep.json"]);
        let actual = prune(&d, 30, "*", false, &["keep.json"]);
        assert_eq!(predicted, actual);
        assert_eq!(actual, 600);
        let _ = fs::remove_dir_all(&d);
    }
}
