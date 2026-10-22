//! LLM model caches. Fixed target list in the `global_cache` style
//! (Wave D5): the directories local LLM tooling downloads models into.
//!
//! Every target is `SafetyLevel::Caution` — deleting a directory whose
//! internals we do not own; the models re-download, but slowly and over
//! the network. The `~/.ollama` ROOT is deliberately NOT a target: keys
//! and config live beside the models. Env-var model dirs (`OLLAMA_MODELS`,
//! `HF_HOME`) are a recorded non-goal — see
//! `docs/decisions/wave-d-coverage.md`.
//!
//! A running Ollama daemon changes the `detail`/`impact` TEXT only, the
//! precedent `claude` sets — it is not a gate: the models on disk are
//! files the daemon re-reads, not live handles.

use super::provider::{
    dir_size_detail_checked, home_dir, impact, measure_dir, CleanupAction, OwnScanner, Provider,
};
use crate::ai_guard::{probe_process, ProcState};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// One model cache we own.
struct Target {
    /// Relative to `$HOME`.
    rel: &'static str,
    label: &'static str,
    regenerates: &'static str,
    /// `pgrep -f` pattern whose running state makes this row say so in its
    /// text. `None` when the tool has no long-lived daemon.
    daemon: Option<&'static str>,
}

/// Fixed list — no user input, ever (G-SAFE-01/02).
const TARGETS: &[Target] = &[
    Target {
        rel: ".ollama/models",
        label: "Ollama models",
        regenerates: "Ollama re-pulls the model on the next `ollama pull`.",
        daemon: Some("ollama"),
    },
    Target {
        rel: ".cache/huggingface/hub",
        label: "Hugging Face cache",
        regenerates: "The next `from_pretrained` re-downloads the weights.",
        daemon: None,
    },
    Target {
        rel: ".lmstudio/models",
        label: "LM Studio models",
        regenerates: "LM Studio re-downloads models you add again.",
        daemon: None,
    },
    Target {
        rel: ".cache/lm-studio/models",
        label: "LM Studio models (legacy)",
        regenerates: "LM Studio re-downloads models you add again.",
        daemon: None,
    },
    Target {
        rel: ".cache/llamafile",
        label: "llamafile cache",
        regenerates: "llamafile re-fetches on the next run.",
        daemon: None,
    },
];

pub struct LlmModelsProvider;

fn path_for(t: &Target) -> Option<PathBuf> {
    let p = home_dir()?.join(t.rel);
    p.is_dir().then_some(p)
}

impl Provider for LlmModelsProvider {
    fn id(&self) -> &'static str {
        "llm_models"
    }
    fn name(&self) -> &'static str {
        "LLM models"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    /// No per-item age: whole model stores, never held back by age (I9).
    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        TARGETS.iter().filter_map(path_for).collect()
    }
}

impl OwnScanner for LlmModelsProvider {
    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        TARGETS
            .iter()
            .filter_map(|t| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let dir = path_for(t)?;
                let size = dir_size_detail_checked(&dir, cancel)?;
                if size.allocated == 0 {
                    return None;
                }
                // The `claude` precedent: a running daemon changes the
                // TEXT only, never the safety and never a gate.
                let daemon_up = t
                    .daemon
                    .is_some_and(|p| probe_process(p) == ProcState::Running);
                Some(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: t.label.to_string(),
                    path: dir,
                    size_bytes: size.allocated,
                    reclaimable_bytes: size.allocated,
                    apparent_bytes: size.apparent_if_material(),
                    inode_bytes: size.inode_bytes.clone(),
                    safety: SafetyLevel::Caution,
                    detail: daemon_up
                        .then(|| "Ollama is running — quit it before cleaning".to_string()),
                    age_days: None,
                    activity: None,
                    last_commit_unix: None,
                    blocked_reason: None,
                    blocked_detail: None,
                    impact: impact(
                        if daemon_up {
                            "Ollama is running right now; quit it first or the next load fails."
                        } else {
                            t.regenerates
                        },
                        Some("Re-downloading models costs significant network and time"),
                        None,
                    ),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_is_home_relative_and_non_empty() {
        for t in TARGETS {
            assert!(!t.rel.starts_with('/'), "{} must be home-relative", t.rel);
            assert!(!t.rel.contains(".."), "{} must not escape HOME", t.rel);
            assert!(!t.regenerates.is_empty(), "{} needs a consequence", t.label);
            // The ollama ROOT holds keys and config: never a target.
            assert_ne!(t.rel, ".ollama");
        }
    }

    /// G-SAFE-03: model stores are directories whose internals we do not
    /// own — `Caution` is the floor, never `Safe`.
    #[test]
    fn nothing_in_this_provider_is_marked_safe() {
        assert_eq!(LlmModelsProvider.default_safety(), SafetyLevel::Caution);
        let cancel = AtomicBool::new(false);
        for r in LlmModelsProvider.scan(&[], &cancel) {
            assert_eq!(
                r.safety,
                SafetyLevel::Caution,
                "{} must not be advertised as Safe",
                r.name
            );
        }
    }

    #[test]
    fn declared_roots_never_include_a_broad_parent() {
        for r in LlmModelsProvider.extra_allowed_roots() {
            let s = r.to_string_lossy();
            assert!(
                !s.ends_with("/.ollama") && !s.ends_with("/.cache"),
                "declaring {s} would widen the validator too far"
            );
        }
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let mut seen = std::collections::HashSet::new();
        for t in TARGETS {
            assert!(seen.insert(t.rel), "duplicate target {}", t.rel);
        }
    }
}
