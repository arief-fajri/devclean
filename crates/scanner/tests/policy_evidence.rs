//! Evidence that the policy gates reach the scan.
//!
//! Before v0.8.0, `Settings.retention_days` had no effect on any cleanup: the
//! backing field was never set to `false` anywhere in production code, so
//! `blocked_bytes` summed a filter that could never match. These
//! tests prove the scan now stamps a reason from the live policy.
//!
//! ## Why every provider here is a stub
//!
//! The first version of this file scanned `default_scan_roots()` on the real
//! machine and asserted that AI resources were held back. That passed locally
//! — this machine has 14 GB of opencode history — and failed on a fresh CI
//! runner, which has never run opencode or Claude. It was asserting "this
//! machine has AI data" while claiming to assert "the AI gate reaches the
//! scan".
//!
//! The wiring under test is: does the scanner consult `is_user_data()` and
//! `Resource::age_days`, and stamp the matching reason? A stub answers that
//! deterministically, on any machine. See G-CI-03.

use devclean_core::{BlockReason, PolicyInputs, Resource, SafetyLevel};
use devclean_providers::{CleanupAction, Provider};
use devclean_scanner::manager::ScanManager;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

fn resource(id: &str, provider: &str, age_days: Option<u64>) -> Resource {
    Resource {
        id: id.to_string(),
        provider: provider.to_string(),
        name: format!("{provider} item"),
        path: PathBuf::from(format!("/nonexistent/{id}")),
        size_bytes: 1024,
        reclaimable_bytes: 1024,
        apparent_bytes: None,
        safety: SafetyLevel::Safe,
        detail: None,
        impact: None,
        age_days,
        activity: None,
        last_commit_unix: None,
        blocked_reason: None,
        blocked_detail: None,
    }
}

/// A provider that declares itself irreplaceable user data. This is the only
/// thing that makes the AI gate apply, and it is a property of the provider —
/// so a stub can exercise it without any real AI history existing.
struct UserDataStub;

impl Provider for UserDataStub {
    fn id(&self) -> &'static str {
        "user_data_stub"
    }
    fn name(&self) -> &'static str {
        "User-data stub"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }
    fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
        vec![resource("ud:0001", self.id(), Some(999))]
    }
    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }
    fn measure(&self, _path: &Path) -> Option<u64> {
        None
    }
    fn is_user_data(&self) -> bool {
        true
    }
}

/// A provider whose items carry an age, for the retention gate. `None` means
/// the provider cannot tell how old its items are — the case the policy must
/// hold back.
struct AgedStub {
    age_days: Option<u64>,
}

impl Provider for AgedStub {
    fn id(&self) -> &'static str {
        "aged_stub"
    }
    fn name(&self) -> &'static str {
        "Aged stub"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Safe
    }
    fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
        vec![resource("ag:0001", self.id(), self.age_days)]
    }
    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }
    fn measure(&self, _path: &Path) -> Option<u64> {
        None
    }
}

fn manager(provider: Arc<dyn Provider>) -> ScanManager {
    ScanManager::with_config(vec![provider], Duration::from_secs(10))
}

/// The gate applies because the provider says so, not because a directory
/// happens to exist on this machine.
#[tokio::test]
async fn the_ai_gate_reaches_the_scan() {
    let mgr = manager(Arc::new(UserDataStub));

    let off = PolicyInputs {
        ai_cleanup_enabled: false,
        ..PolicyInputs::default()
    };
    let s = mgr.scan_all(vec![], &[], &[], off, None, None).await;
    assert_eq!(s.resources.len(), 1, "the stub must produce one resource");
    assert_eq!(
        s.resources[0].blocked_reason,
        Some(BlockReason::AiReadOnly),
        "AI data must be report-only while the setting is off"
    );
    assert!(
        s.resources[0]
            .blocked_detail
            .as_deref()
            .is_some_and(|d| !d.is_empty()),
        "a blocked resource must explain itself"
    );
    assert_eq!(
        s.blocked_bytes, 1024,
        "held-back bytes must be reported separately, not silently dropped"
    );

    let on = PolicyInputs {
        ai_cleanup_enabled: true,
        ..PolicyInputs::default()
    };
    let s = mgr.scan_all(vec![], &[], &[], on, None, None).await;
    assert_eq!(
        s.resources[0].blocked_reason, None,
        "enabling AI cleanup must make it selectable"
    );
    assert_eq!(s.blocked_bytes, 0);
}

/// The regression this whole module exists for: a retention policy that had no
/// effect on anything.
#[tokio::test]
async fn retention_reaches_the_scan() {
    let mgr = manager(Arc::new(AgedStub { age_days: Some(1) }));

    let keep_30 = PolicyInputs {
        retention_days: Some(30),
        ..PolicyInputs::default()
    };
    let s = mgr.scan_all(vec![], &[], &[], keep_30, None, None).await;
    assert_eq!(
        s.resources[0].blocked_reason,
        Some(BlockReason::Retention),
        "a 1-day-old item must be held back by a 30-day policy"
    );
    let detail = s.resources[0].blocked_detail.clone().unwrap_or_default();
    assert!(detail.contains("30-day"), "got: {detail}");
    assert!(detail.contains("1d old"), "got: {detail}");

    let no_policy = PolicyInputs {
        retention_days: None,
        ..PolicyInputs::default()
    };
    let s = mgr.scan_all(vec![], &[], &[], no_policy, None, None).await;
    assert_eq!(
        s.resources[0].blocked_reason, None,
        "with no policy, nothing is held back"
    );
}

/// An unknown age is held back: when in doubt, keep the data. A provider that
/// cannot say how old its items are must not get the benefit of the doubt.
#[tokio::test]
async fn an_unknown_age_is_held_back_by_a_retention_policy() {
    let mgr = manager(Arc::new(AgedStub { age_days: None }));
    let keep_30 = PolicyInputs {
        retention_days: Some(30),
        ..PolicyInputs::default()
    };
    let s = mgr.scan_all(vec![], &[], &[], keep_30, None, None).await;
    assert_eq!(s.resources[0].blocked_reason, Some(BlockReason::Retention));
    assert!(
        s.resources[0]
            .blocked_detail
            .as_deref()
            .is_some_and(|d| d.contains("unknown")),
        "the reason must say the age is unknown, not invent one"
    );
}

/// An old enough item passes the same policy, so the gate is a cutoff and not
/// a blanket refusal.
#[tokio::test]
async fn an_old_enough_item_passes_the_retention_policy() {
    let mgr = manager(Arc::new(AgedStub { age_days: Some(31) }));
    let keep_30 = PolicyInputs {
        retention_days: Some(30),
        ..PolicyInputs::default()
    };
    let s = mgr.scan_all(vec![], &[], &[], keep_30, None, None).await;
    assert_eq!(s.resources[0].blocked_reason, None);
}

/// Authorisation outranks age: a user who has not enabled AI cleanup must be
/// told that, not told about a retention window that would not have mattered.
#[tokio::test]
async fn the_ai_gate_outranks_the_retention_gate() {
    let mgr = manager(Arc::new(UserDataStub));
    let both = PolicyInputs {
        ai_cleanup_enabled: false,
        retention_days: Some(3650),
        ..PolicyInputs::default()
    };
    let s = mgr.scan_all(vec![], &[], &[], both, None, None).await;
    assert_eq!(
        s.resources[0].blocked_reason,
        Some(BlockReason::AiReadOnly),
        "the setting that actually matters must be the one reported"
    );
}
