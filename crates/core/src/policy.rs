//! One place that decides why a resource cannot be cleaned (I11).
//!
//! Before v0.8.0 this logic was spread across the engine and, in practice,
//! never ran at scan time at all: `Resource.retention_eligible` was only ever
//! written as `true`, so `Settings.retention_days` had a UI control, a
//! contract field and an ADR but **no effect on any cleanup**. The scanner's
//! `retention_blocked_bytes` summed a filter that could never match.
//!
//! The fix is structural: the scan stamps `blocked_reason` on every resource
//! and the engine refuses whatever is stamped, both by calling
//! [`block_reason_for`]. The reason the user reads is the reason that is
//! enforced, because there is only one function.
//!
//! ## Why this lives in `core`
//!
//! The scanner and the cleanup engine both need it, and `core` sits below
//! both. It takes plain values rather than a `Provider`, so it stays free of
//! the provider crate (and of any dependency cycle).

use crate::{BlockReason, DockerPruneScope, Resource};

/// Everything the decision depends on. Built from `Settings` plus the
/// provider's own declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyInputs {
    /// The provider says its resources are irreplaceable user data.
    pub is_user_data: bool,
    /// The narrowest Docker scope the provider needs, or `None` when it does
    /// not touch Docker at all.
    pub required_docker_scope: Option<DockerPruneScope>,
    /// `Settings.retention_days`.
    pub retention_days: Option<u32>,
    /// `Settings.docker_prune_scope`.
    pub docker_prune_scope: DockerPruneScope,
    /// `Settings.allow_privileged_operations`.
    pub allow_privileged_operations: bool,
    /// `Settings.ai_cleanup_enabled`.
    pub ai_cleanup_enabled: bool,
    /// Git-aware protection (v0.10.0): the artifact is tracked by a
    /// repository that has uncommitted changes. Deleting it would lose work
    /// that is not committed anywhere, so this outranks every other gate.
    /// Set by the scanner after the git pass; `false` for anything that is
    /// not classified.
    pub protected: bool,
    /// A browser whose code-sign clones this resource holds is confirmed
    /// running (v0.13.0, Wave D4). The raw probe result, ANDed by the
    /// scanner with the provider's `held_while_browsers_run` declaration so
    /// only browser-temp rows are ever subject to it. `false` means "not
    /// confirmed running" — a FAILED probe (`ProcState::Unknown`) never
    /// blocks (G-SAFE-07); it floors the safety instead, at scan time.
    pub browser_running: bool,
}

impl Default for PolicyInputs {
    /// A permissive policy: nothing blocked. Used by the ungated test path.
    fn default() -> Self {
        Self {
            is_user_data: false,
            required_docker_scope: None,
            retention_days: None,
            docker_prune_scope: DockerPruneScope::BuildCache,
            allow_privileged_operations: true,
            ai_cleanup_enabled: true,
            protected: false,
            browser_running: false,
        }
    }
}

/// Why `r` may not be cleaned, with the sentence to show. `None` means it may.
///
/// Order matters and is deliberate: git protection outranks everything — a
/// tracked file in a dirty repository is data loss whatever the other
/// settings say. After it, an *authorisation* gate (AI report-only, Docker
/// scope, privileged opt-in, a running browser) outranks an *age* gate.
/// Telling a user "kept by your 30-day policy" about history they have not
/// enabled at all would hide the setting that actually matters.
pub fn block_reason_for(r: &Resource, p: PolicyInputs) -> Option<(BlockReason, String)> {
    // 0. Git-aware protection: deleting a tracked artifact from a dirty
    //    repository loses uncommitted work. This is data loss regardless of
    //    every other setting, so it is checked first.
    if p.protected {
        return Some((
            BlockReason::Protected,
            "tracked by a repository with uncommitted changes — commit or stash first".to_string(),
        ));
    }

    // 1. AI conversation history: report-only until explicitly enabled.
    if p.is_user_data && !p.ai_cleanup_enabled {
        return Some((
            BlockReason::AiReadOnly,
            "AI history is report-only until you enable it in Settings > Cleanup".to_string(),
        ));
    }

    // 2. Docker: scope off, a missing privileged opt-in, or a scope
    //    narrower than the provider declared. Only providers that declare a
    //    Docker scope are subject to this; everything else passes straight
    //    through.
    if let Some(required) = p.required_docker_scope {
        if p.docker_prune_scope == DockerPruneScope::Off {
            return Some((
                BlockReason::DockerScope,
                "Docker cleanup is off in Settings > Cleanup".to_string(),
            ));
        }
        if required.requires_privileged() && !p.allow_privileged_operations {
            return Some((
                BlockReason::PrivilegedRequired,
                format!(
                    "needs Privileged operations in Settings > Cleanup ({})",
                    required.consequence()
                ),
            ));
        }
        // The scope must also BE wide enough. Before v1.1.0 only `Off` and
        // the privileged flag were checked, so a `build-cache` selection
        // with the privileged opt-in on still let the image/volume prune
        // run — a narrower scope silently did nothing.
        if p.docker_prune_scope < required {
            return Some((
                BlockReason::DockerScopeRequired,
                format!(
                    "needs Docker scope '{}' in Settings > Cleanup",
                    required.label()
                ),
            ));
        }
    }

    // 3. A browser that owns these code-sign clones is running. An
    //    authorisation gate, so it outranks the age gate below; only the
    //    browser-temp provider is ever subject to it (the scanner ANDs the
    //    probe with the provider's declaration). A FAILED probe never
    //    reaches here — only a confirmed `Running` does (G-SAFE-07).
    if p.browser_running {
        return Some((
            BlockReason::BrowserRunning,
            "the browser is running — quit it and rescan to clean its temporary files".to_string(),
        ));
    }

    // 4. Age policy. An unknown age is held back: when in doubt, keep the
    //    data. `is_none_or` reads as "no age, or too young".
    if let Some(keep) = p.retention_days {
        if r.age_days.is_none_or(|d| d <= u64::from(keep)) {
            return Some((
                BlockReason::Retention,
                match r.age_days {
                    Some(d) => {
                        format!("kept by your {keep}-day retention policy (this is {d}d old)")
                    }
                    None => format!("kept by your {keep}-day retention policy (age unknown)"),
                },
            ));
        }
    }

    None
}

/// Stamp `blocked_reason`/`blocked_detail` on a resource. Idempotent: a reason
/// already set by a provider is never overwritten.
pub fn stamp_blocked_reason(r: &mut Resource, p: PolicyInputs) {
    if r.blocked_reason.is_some() {
        return;
    }
    if let Some((reason, detail)) = block_reason_for(r, p) {
        r.blocked_reason = Some(reason);
        r.blocked_detail = Some(detail);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SafetyLevel, Sizing};
    use std::path::PathBuf;

    fn res(age_days: Option<u64>) -> Resource {
        Resource {
            id: "x:1".into(),
            provider: "p".into(),
            name: "n".into(),
            path: PathBuf::from("/tmp/x"),
            size_bytes: 1,
            reclaimable_bytes: 1,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: None,
            age_days,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        }
    }

    fn permissive() -> PolicyInputs {
        PolicyInputs::default()
    }

    #[test]
    fn a_permissive_policy_blocks_nothing() {
        assert_eq!(block_reason_for(&res(Some(0)), permissive()), None);
    }

    /// The bug this module exists to prevent: `Settings.retention_days` had no
    /// effect because nothing ever stamped a resource as kept.
    #[test]
    fn retention_actually_blocks_a_young_resource() {
        let p = PolicyInputs {
            retention_days: Some(30),
            ..permissive()
        };
        let (reason, detail) = block_reason_for(&res(Some(3)), p).expect("must block");
        assert_eq!(reason, BlockReason::Retention);
        assert!(detail.contains("30-day"), "got: {detail}");
        assert!(detail.contains("3d old"), "got: {detail}");
    }

    #[test]
    fn retention_allows_an_old_enough_resource() {
        let p = PolicyInputs {
            retention_days: Some(30),
            ..permissive()
        };
        assert_eq!(block_reason_for(&res(Some(31)), p), None);
    }

    #[test]
    fn retention_holds_back_an_unknown_age() {
        let p = PolicyInputs {
            retention_days: Some(30),
            ..permissive()
        };
        let (reason, detail) = block_reason_for(&res(None), p).expect("must block");
        assert_eq!(reason, BlockReason::Retention);
        assert!(detail.contains("age unknown"), "got: {detail}");
    }

    /// Authorisation outranks age: a user who has not enabled AI cleanup must
    /// be told that, not told about a retention window.
    #[test]
    fn ai_read_only_outranks_retention() {
        let p = PolicyInputs {
            is_user_data: true,
            ai_cleanup_enabled: false,
            retention_days: Some(30),
            ..permissive()
        };
        let (reason, _) = block_reason_for(&res(Some(1)), p).expect("must block");
        assert_eq!(reason, BlockReason::AiReadOnly);
    }

    /// Git protection outranks everything: a tracked artifact in a dirty
    /// repository is data loss regardless of the AI, Docker, browser or age
    /// settings.
    #[test]
    fn git_protection_outranks_every_other_gate() {
        let p = PolicyInputs {
            protected: true,
            is_user_data: true,
            ai_cleanup_enabled: false,
            retention_days: Some(30),
            browser_running: true,
            ..permissive()
        };
        let (reason, detail) = block_reason_for(&res(Some(1)), p).expect("must block");
        assert_eq!(reason, BlockReason::Protected);
        assert!(detail.contains("uncommitted"), "got: {detail}");
    }

    /// Wave D4: a confirmed-running browser blocks the browser-temp rows,
    /// with a sentence naming the action the user must take.
    #[test]
    fn a_running_browser_blocks_the_resource() {
        let p = PolicyInputs {
            browser_running: true,
            ..permissive()
        };
        let (reason, detail) = block_reason_for(&res(Some(999)), p).expect("must block");
        assert_eq!(reason, BlockReason::BrowserRunning);
        assert!(detail.contains("quit"), "got: {detail}");
        // Not confirmed running: nothing blocked.
        assert_eq!(block_reason_for(&res(Some(999)), permissive()), None);
    }

    /// The browser gate is an authorisation gate: it outranks the age gate,
    /// like the AI and Docker gates do.
    #[test]
    fn browser_running_outranks_retention() {
        let p = PolicyInputs {
            browser_running: true,
            retention_days: Some(30),
            ..permissive()
        };
        let (reason, _) = block_reason_for(&res(Some(1)), p).expect("must block");
        assert_eq!(reason, BlockReason::BrowserRunning);
    }

    #[test]
    fn enabling_ai_cleanup_unblocks_it() {
        let p = PolicyInputs {
            is_user_data: true,
            ai_cleanup_enabled: true,
            ..permissive()
        };
        assert_eq!(block_reason_for(&res(Some(999)), p), None);
    }

    #[test]
    fn docker_off_blocks_only_providers_that_use_docker() {
        let p = PolicyInputs {
            docker_prune_scope: DockerPruneScope::Off,
            ..permissive()
        };
        // A provider that does not touch Docker is unaffected.
        assert_eq!(block_reason_for(&res(Some(999)), p), None);
        // One that does is blocked.
        let p2 = PolicyInputs {
            required_docker_scope: Some(DockerPruneScope::BuildCache),
            ..p
        };
        let (reason, _) = block_reason_for(&res(Some(999)), p2).expect("must block");
        assert_eq!(reason, BlockReason::DockerScope);
    }

    #[test]
    fn a_wider_docker_scope_needs_the_privileged_opt_in() {
        let p = PolicyInputs {
            required_docker_scope: Some(DockerPruneScope::All),
            docker_prune_scope: DockerPruneScope::All,
            allow_privileged_operations: false,
            ..permissive()
        };
        let (reason, detail) = block_reason_for(&res(Some(999)), p).expect("must block");
        assert_eq!(reason, BlockReason::PrivilegedRequired);
        assert!(detail.contains("Privileged"), "got: {detail}");

        let allowed = PolicyInputs {
            allow_privileged_operations: true,
            ..p
        };
        assert_eq!(block_reason_for(&res(Some(999)), allowed), None);
    }

    /// The gap this closes: with the privileged opt-in on, a `build-cache`
    /// scope still let `docker_pruned` (which needs `all`) run. Turning the
    /// opt-in on is not the same as choosing a wide enough scope.
    #[test]
    fn a_scope_narrower_than_the_provider_needs_is_refused() {
        let p = PolicyInputs {
            required_docker_scope: Some(DockerPruneScope::All),
            docker_prune_scope: DockerPruneScope::BuildCache,
            allow_privileged_operations: true,
            ..permissive()
        };
        let (reason, detail) = block_reason_for(&res(Some(999)), p).expect("must block");
        assert_eq!(reason, BlockReason::DockerScopeRequired);
        assert!(
            detail.contains("'all'"),
            "the detail must name the scope to choose, got: {detail}"
        );

        // The same provider at its required scope passes.
        let wide = PolicyInputs {
            docker_prune_scope: DockerPruneScope::All,
            ..p
        };
        assert_eq!(block_reason_for(&res(Some(999)), wide), None);

        // And a WIDER scope than required is not punished.
        let wider = PolicyInputs {
            docker_prune_scope: DockerPruneScope::All,
            required_docker_scope: Some(DockerPruneScope::BuildCache),
            ..p
        };
        assert_eq!(block_reason_for(&res(Some(999)), wider), None);
    }

    #[test]
    fn stamping_is_idempotent_and_preserves_a_provider_set_reason() {
        let p = PolicyInputs {
            retention_days: Some(30),
            ..permissive()
        };
        let mut r = res(Some(1));
        stamp_blocked_reason(&mut r, p);
        assert_eq!(r.blocked_reason, Some(BlockReason::Retention));
        let detail = r.blocked_detail.clone();

        // A provider that already said "protected" keeps its reason.
        let mut protected = res(Some(1));
        protected.blocked_reason = Some(BlockReason::Protected);
        protected.blocked_detail = Some("tracked file in a dirty repo".into());
        stamp_blocked_reason(&mut protected, p);
        assert_eq!(protected.blocked_reason, Some(BlockReason::Protected));
        assert_eq!(
            protected.blocked_detail.as_deref(),
            Some("tracked file in a dirty repo")
        );

        // Re-stamping does not change an existing verdict.
        stamp_blocked_reason(&mut r, permissive());
        assert_eq!(r.blocked_detail, detail);
    }

    #[test]
    fn every_reason_has_a_label() {
        for r in [
            BlockReason::Retention,
            BlockReason::AiReadOnly,
            BlockReason::PrivilegedRequired,
            BlockReason::DockerScope,
            BlockReason::DockerScopeRequired,
            BlockReason::Protected,
            BlockReason::BrowserRunning,
        ] {
            assert!(!r.label().is_empty());
        }
    }

    #[test]
    fn sizing_defaults_to_apparent_for_old_history_lines() {
        // Every entry written before v0.8.0 summed file lengths.
        assert_eq!(Sizing::default(), Sizing::Apparent);
    }
}
