//! opencode filesystem data (`~/.local/share/opencode`).
//!
//! opencode writes logs, session diffs, file snapshots and tool output
//! next to its SQLite history. Unlike a build cache, none of it is needed
//! to *run* opencode — it is conversation history, which is exactly why it
//! is age-pruned rather than wiped (I9, I10).
//!
//! Hard rules enforced here:
//! - Nothing is touched while opencode is running (`ai_guard::opencode_running`).
//! - `opencode.db` is NOT this provider's job — see `opencode_db`.
//! - Config and credentials are never prune targets (I10).

use super::ai_guard;
use super::provider::{dir_size_detail, home_dir, impact, project_detail, CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// (id, label, subdir, pattern, prune empty dirs?) — fixed strings only.
const TARGETS: &[(&str, &str, &str, &str, bool)] = &[
    ("log", "Logs", "log", "*.log", false),
    (
        "session_diff",
        "Session diffs",
        "storage/session_diff",
        "*.json",
        true,
    ),
    ("snapshot", "Snapshots", "snapshot", "*", true),
    ("tool_output", "Tool output", "tool-output", "*", false),
    ("plans", "Plans", "plans", "*.md", false),
];

pub struct OpencodeFsProvider;

pub fn opencode_dir() -> Option<PathBuf> {
    let dir = home_dir()?.join(".local/share/opencode");
    dir.is_dir().then_some(dir)
}

impl Provider for OpencodeFsProvider {
    fn id(&self) -> &'static str {
        "opencode_fs"
    }
    fn name(&self) -> &'static str {
        "opencode data"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }

    fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
        let Some(base) = opencode_dir() else {
            return Vec::new();
        };
        // Hint only; see `ai_guard`. The real protection for these paths
        // is the age cutoff, not the process probe.
        let running = ai_guard::probe_process("opencode").is_running();
        TARGETS
            .iter()
            .filter_map(|(_id, label, sub, _pat, _empty)| {
                let dir = base.join(sub);
                if !dir.is_dir() {
                    return None;
                }
                let size = dir_size_detail(&dir);
                if size.allocated == 0 {
                    return None;
                }
                Some(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: format!("opencode {label}"),
                    path: dir.clone(),
                    size_bytes: size.allocated,
                    reclaimable_bytes: size.allocated,
                    apparent_bytes: size.apparent_if_material(),
                    safety: SafetyLevel::Caution,
                    detail: Some(if running {
                        "opencode is running — cleanup will be skipped".to_string()
                    } else {
                        detail_for(&dir)
                    }),
                    age_days: super::provider::age_days(&dir),
                    activity: None,
                    last_commit_unix: None,
                    blocked_reason: None,
                    blocked_detail: None,
                    impact: impact(
                        if running {
                            "opencode is running right now, so this is skipped. \
                             Quit opencode and clean it then."
                        } else {
                            "Conversation history only; opencode itself keeps working."
                        },
                        Some("Old sessions can no longer be resumed"),
                        None,
                    ),
                })
            })
            .collect()
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        let (pattern, empty) = TARGETS
            .iter()
            .find(|(_, _, sub, _, _)| path.ends_with(sub))
            .map(|(_, _, _, p, e)| (*p, *e))
            .unwrap_or(("*", false));
        CleanupAction::PruneOlderThan {
            dir: path.to_path_buf(),
            // 0 = the user's retention policy decides; the engine applies
            // `retention_days` before this action can ever run.
            min_age_days: 0,
            pattern,
            prune_empty_dirs: empty,
            skip_names: ai_guard::NEVER_PRUNED,
        }
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        super::provider::measure_dir(path)
    }

    /// opencode keeps a single `log/` dir rather than per-item history, so
    /// a whole-directory age is the only signal available. It still counts:
    /// a log dir untouched for months is safe to clear.
    fn is_user_data(&self) -> bool {
        true
    }

    fn retention_days(&self) -> Option<u32> {
        Some(30)
    }

    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        opencode_dir().into_iter().collect()
    }
}

fn detail_for(dir: &Path) -> String {
    project_detail(
        &dir.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dir,
        0,
    )
}
