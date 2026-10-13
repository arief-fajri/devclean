//! The `Provider` trait: the single extension point for new resource kinds.
//!
//! To add a provider (e.g. Python cache): implement this trait, add it to
//! [`crate::all_providers`], done. No `if/else` chains elsewhere.

use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// How a resource must be removed. Providers declare *what* removal means;
/// the cleanup engine + safety validator decide *whether* it may run
/// (guardrails G-SAFE-01..04).
#[derive(Debug, Clone)]
pub enum CleanupAction {
    /// Delete the directory tree (e.g. `node_modules`, `target/`).
    RemoveDir(PathBuf),
    /// Run an official tool with fixed args (e.g. `go clean -cache`).
    /// Never constructed from user input (G-SAFE-02).
    RunCommand {
        program: &'static str,
        args: &'static [&'static str],
    },
}

pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn default_safety(&self) -> SafetyLevel;

    /// Find resources under `roots`. Returned [`Resource`]s carry `id == ""`;
    /// the [`crate::ProviderRegistry`] assigns opaque ids after scanning so
    /// providers never invent the security-sensitive identifier.
    ///
    /// `cancel` is checked cooperatively (at least once per directory
    /// entry); when set, return whatever has been found so far so the scan
    /// settles in <1s (G-SCAN-02).
    fn scan(&self, roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource>;

    /// How `path` (previously returned by [`Provider::scan`]) must be
    /// removed. Called only after the safety validator approves the path.
    fn cleanup_action(&self, path: &Path) -> CleanupAction;

    /// Global cache directories this provider owns outside scan roots
    /// (e.g. `~/.cargo/registry`). The backend adds these to the validator's
    /// allowed roots; default is none. Only ever constant paths derived
    /// server-side — never user input (G-SAFE-01).
    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        Vec::new()
    }
}

/// Sum of file sizes under `path`. Symlinks are not followed (a symlink farm
/// must never explode a scan, nor cause deletion outside the tree).
pub fn dir_size(path: &Path) -> u64 {
    static NEVER: AtomicBool = AtomicBool::new(false);
    dir_size_checked(path, &NEVER).unwrap_or(0)
}

/// Cancellable variant of [`dir_size`]. Returns `None` when `cancel` was
/// set mid-measurement so callers can abort promptly (G-SCAN-02).
pub fn dir_size_checked(path: &Path, cancel: &AtomicBool) -> Option<u64> {
    let mut total = 0u64;
    for e in walkdir::WalkDir::new(path).follow_links(false).into_iter() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let Ok(e) = e else { continue };
        let Ok(m) = e.metadata() else { continue };
        if m.is_file() {
            total += m.len();
        }
    }
    Some(total)
}

/// Home directory (`HOME`, fallback `USERPROFILE` on Windows).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Run `<program> <args>` and parse stdout as a path (e.g. `pnpm store
/// path`, `yarn cache dir`). Returns `None` when the tool is missing —
/// absence is normal, not an error. Fixed args only (G-SAFE-02).
pub fn tool_output_dir(program: &str, args: &[&str]) -> Option<PathBuf> {
    let text = run_tool(program, args)?;
    let p = PathBuf::from(text.trim());
    if p.as_os_str().is_empty() {
        return None;
    }
    Some(p)
}

fn run_tool(program: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
}

/// Shared walk helper: iterate directory entries under `root` up to
/// `max_depth`, aborting promptly when `cancel` is set. Symlinks are
/// never followed (G-SCAN-03).
pub fn walk_cancelled(
    root: &Path,
    max_depth: usize,
    cancel: &AtomicBool,
    mut visit: impl FnMut(&walkdir::DirEntry),
) {
    let iter = walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth)
        .into_iter();
    for entry in iter {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(e) = entry else { continue };
        visit(&e);
    }
}

/// Age of `path` in whole days, from filesystem mtime. This is a display
/// signal only — never a safety verdict (a stale project is not automatically
/// safe to delete; the validator still decides).
pub fn age_days(path: &Path) -> Option<u64> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let age = std::time::SystemTime::now()
        .duration_since(mtime)
        .unwrap_or_default();
    Some(age.as_secs() / 86_400)
}

/// Human age suffix for the `detail` string, e.g. `" · 143d old"`.
/// Empty when the mtime is unreadable.
pub fn age_suffix(path: &Path) -> String {
    match age_days(path) {
        Some(d) if d < 30 => format!(" · {d}d old"),
        Some(d) if d < 365 => format!(" · {}mo old", d / 30),
        Some(d) => format!(" · {}y old", d / 365),
        None => String::new(),
    }
}

/// Smart stale assessment: combines project age with resource size into a
/// reclaim hint. This is a SIGNAL ONLY — never a safety verdict. The safety
/// validator still decides what may be deleted, and every cleanup still
/// passes preview + confirmation.
pub fn stale_note(project: &Path, bytes: u64) -> Option<String> {
    stale_note_for_age(age_days(project), bytes)
}

/// Pure rules behind [`stale_note`], unit-tested at the boundaries.
pub fn stale_note_for_age(age_days: Option<u64>, bytes: u64) -> Option<String> {
    const GB: u64 = 1_000_000_000;
    match age_days? {
        age if age > 90 && bytes > GB / 2 => {
            Some("stale & large — strong reclaim candidate (signal only)".to_string())
        }
        age if age > 90 => Some(
            "stale — untouched for a long time (signal only, never a safety verdict)".to_string(),
        ),
        age if age > 30 && bytes > GB => Some("large & aging (signal only)".to_string()),
        _ => None,
    }
}

/// Full `detail` string for filesystem project caches: name, age signal,
/// and optional stale note.
pub fn project_detail(project_name: &str, project: &Path, bytes: u64) -> String {
    let stale = stale_note(project, bytes)
        .map(|s| format!(" · {s}"))
        .unwrap_or_default();
    format!("project: {project_name}{}{stale}", age_suffix(project))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_of_fresh_dir_is_zero_days() {
        let dir = std::env::temp_dir().join(format!("devclean-age-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(age_days(&dir), Some(0));
        assert_eq!(age_suffix(&dir), " · 0d old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn age_of_missing_path_is_none() {
        let missing = std::path::PathBuf::from("/definitely/not/a/devclean/path");
        assert_eq!(age_days(&missing), None);
        assert_eq!(age_suffix(&missing), "");
    }

    #[test]
    fn stale_rules_fire_only_at_the_boundaries() {
        // Unknown age → no signal.
        assert_eq!(stale_note_for_age(None, u64::MAX), None);
        // Young and small → no signal.
        assert_eq!(stale_note_for_age(Some(10), 100), None);
        // Old but small → stale (not large).
        let note = stale_note_for_age(Some(91), 100).unwrap();
        assert!(note.contains("stale") && !note.contains("large"));
        // Old and large → strongest signal.
        let note = stale_note_for_age(Some(200), 600_000_000).unwrap();
        assert!(note.contains("stale & large"));
        // Boundary: exactly 90 days is aging when huge, never "stale".
        let note = stale_note_for_age(Some(90), u64::MAX).unwrap();
        assert!(!note.contains("stale"));
        // Young but huge → aging (not stale).
        let note = stale_note_for_age(Some(31), 2_000_000_000).unwrap();
        assert!(note.contains("aging") && !note.contains("stale"));
    }

    #[test]
    fn project_detail_combines_name_age_and_stale_note() {
        let dir = std::env::temp_dir().join(format!("devclean-pd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let detail = project_detail("my-app", &dir, 100);
        assert!(detail.starts_with("project: my-app"));
        assert!(detail.contains("old"));
        assert!(!detail.contains("stale"), "fresh dir must not hint stale");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
