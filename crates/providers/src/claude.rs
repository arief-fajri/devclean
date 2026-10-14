//! Claude Code data (`~/.claude`).
//!
//! Conversation transcripts, per-file edit history and tool-result caches.
//! Same shape as the opencode filesystem provider: age-pruned, never wiped
//! wholesale, and never while Claude Code is running.
//!
//! I10 is the important one: `history.jsonl` (the global prompt history)
//! and every settings/credentials file are on a hard-preserve list and are
//! never prune targets, whatever the retention policy says. The legacy
//! `cleanup.sh` preserves `history.jsonl` for the same reason.

use super::ai_guard;
use super::provider::{dir_size_detail, home_dir, impact, measure_dir, CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// (id, label, subdir, pattern, prune empty dirs?) — fixed strings only.
/// `projects/` holds one subdir per conversation with nested jsonl files,
/// so it is the only target that prunes empty directories.
const TARGETS: &[(&str, &str, &str, &str, bool)] = &[
    ("projects", "Conversation history", "projects", "*", true),
    ("file_history", "File history", "file-history", "*", true),
    ("backups", "Backups", "backups", "*", false),
];

/// `~/.claude/cache/` holds changelogs and similar, which are regenerable
/// on the next launch, so it is cleared entirely rather than age-pruned.
/// Still refused while Claude Code is running.
const CACHE_SUBDIR: &str = "cache";

pub struct ClaudeProvider;

pub fn claude_dir() -> Option<PathBuf> {
    let dir = home_dir()?.join(".claude");
    dir.is_dir().then_some(dir)
}

impl Provider for ClaudeProvider {
    fn id(&self) -> &'static str {
        "claude"
    }
    fn name(&self) -> &'static str {
        "Claude data"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }

    fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
        let Some(base) = claude_dir() else {
            return Vec::new();
        };
        // Hint only — see `ai_guard`: a sandboxed probe cannot see the
        // process table, and must never be read as "safe".
        let running = ai_guard::probe_process("claude").is_running();
        let mut out: Vec<Resource> = TARGETS
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
                    name: format!("Claude {label}"),
                    path: dir,
                    size_bytes: size.allocated,
                    reclaimable_bytes: size.allocated,
                    apparent_bytes: size.apparent_if_material(),
                    safety: SafetyLevel::Caution,
                    detail: Some(if running {
                        "Claude Code is running — cleanup will be skipped".to_string()
                    } else {
                        String::new()
                    }),
                    // The directory's own mtime is recent whenever anything inside
                    // is touched, so this under-estimates age. That is the SAFE
                    // direction: it errs toward holding data back, never
                    // toward deleting it early.
                    age_days: super::provider::age_days(&base.join(sub)),
                    blocked_reason: None,
                    blocked_detail: None,
                    impact: impact(
                        if running {
                            "Claude Code is running right now, so this is skipped."
                        } else {
                            "Old conversations only; your settings and credentials are untouched."
                        },
                        Some("Old conversations can no longer be resumed"),
                        None,
                    ),
                })
            })
            .collect();

        // Regenerable launcher cache: cleared whole, but only when Claude is
        // not running.
        let cache = base.join(CACHE_SUBDIR);
        if cache.is_dir() {
            let size = dir_size_detail(&cache);
            if size.allocated > 0 {
                out.push(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: "Claude cache".to_string(),
                    path: cache,
                    size_bytes: size.allocated,
                    reclaimable_bytes: size.allocated,
                    apparent_bytes: size.apparent_if_material(),
                    safety: SafetyLevel::Safe,
                    detail: Some(if running {
                        "Claude Code is running — cleanup will be skipped".to_string()
                    } else {
                        "Regenerated on the next launch".to_string()
                    }),
                    age_days: None,
                    blocked_reason: None,
                    blocked_detail: None,
                    impact: impact("Regenerated by Claude Code on the next launch.", None, None),
                });
            }
        }
        out
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        if path.ends_with(CACHE_SUBDIR) {
            // Regenerable: a plain directory wipe is correct and simpler
            // than an age pass over a cache nobody reads.
            return CleanupAction::RemoveDir(path.to_path_buf());
        }
        let (pattern, empty) = TARGETS
            .iter()
            .find(|(_, _, sub, _, _)| path.ends_with(sub))
            .map(|(_, _, _, p, e)| (*p, *e))
            .unwrap_or(("*", false));
        CleanupAction::PruneOlderThan {
            dir: path.to_path_buf(),
            min_age_days: 0,
            pattern,
            prune_empty_dirs: empty,
            // I10: `projects/` holds one subdir per conversation whose names
            // are opaque hashes, so a stray `settings.json` there must still
            // survive a `*` pattern.
            skip_names: ai_guard::NEVER_PRUNED,
        }
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn is_user_data(&self) -> bool {
        true
    }

    fn retention_days(&self) -> Option<u32> {
        Some(30)
    }

    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        claude_dir().into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_dir_is_home_relative_when_present() {
        if let Some(dir) = claude_dir() {
            assert!(dir.ends_with(".claude"));
            assert_eq!(ClaudeProvider.extra_allowed_roots(), vec![dir]);
        }
    }

    #[test]
    fn cache_subdir_maps_to_a_full_wipe_not_an_age_pass() {
        let p = PathBuf::from("/tmp/.claude/cache");
        assert!(matches!(
            ClaudeProvider.cleanup_action(&p),
            CleanupAction::RemoveDir(_)
        ));
        let p = PathBuf::from("/tmp/.claude/projects");
        assert!(matches!(
            ClaudeProvider.cleanup_action(&p),
            CleanupAction::PruneOlderThan {
                prune_empty_dirs: true,
                ..
            }
        ));
    }

    /// I10: the prune pattern is `*`, so the hard-preserve list in
    /// `ai_guard` is what stops `history.jsonl` from matching. This test
    /// pins that both files sit side by side in the same directory.
    #[test]
    fn history_jsonl_would_match_the_pattern_but_is_preserved() {
        assert!(
            ai_guard::is_never_pruned("history.jsonl"),
            "the preserve list is the only thing protecting it from `*`"
        );
        assert_eq!(ai_guard::NEVER_PRUNED.len(), 6);
    }
}
