use devclean_core::{BlockReason, DockerPruneScope, PolicyInputs};
use devclean_scanner::manager::ScanManager;
use std::path::PathBuf;

#[tokio::test]
async fn retention_and_ai_gates_now_reach_the_scan() {
    let roots = devclean_core::default_scan_roots();
    if roots.is_empty() {
        return;
    }

    // Before v0.8.0 this produced ZERO blocked resources for any policy,
    // because nothing ever stamped a reason at scan time.
    let strict = PolicyInputs {
        retention_days: Some(3650),
        ai_cleanup_enabled: false,
        ..PolicyInputs::default()
    };
    let mgr = ScanManager::default();
    let s = mgr
        .scan_all(roots.clone(), &[], &[], strict, None, None)
        .await;

    let blocked = s
        .resources
        .iter()
        .filter(|r| r.blocked_reason.is_some())
        .count();
    let by_reason = |want: BlockReason| {
        s.resources
            .iter()
            .filter(|r| r.blocked_reason == Some(want))
            .count()
    };
    println!("\n  roots            : {}", roots.len());
    println!("  resources        : {}", s.resources.len());
    println!("  blocked          : {blocked}");
    println!("  retention        : {}", by_reason(BlockReason::Retention));
    println!(
        "  ai read-only     : {}",
        by_reason(BlockReason::AiReadOnly)
    );
    println!("  blocked bytes    : {} B", s.retention_blocked_bytes);

    assert!(
        blocked > 0,
        "a 10-year retention policy must hold something back"
    );
    assert!(
        by_reason(BlockReason::AiReadOnly) > 0,
        "AI data must be report-only by default"
    );

    // Permissive policy: nothing blocked.
    let open = PolicyInputs {
        retention_days: None,
        ai_cleanup_enabled: true,
        ..PolicyInputs::default()
    };
    let s2 = mgr.scan_all(roots, &[], &[], open, None, None).await;
    assert_eq!(
        s2.resources
            .iter()
            .filter(|r| r.blocked_reason.is_some())
            .count(),
        0,
        "with no policy and AI enabled, nothing should be held back"
    );
    let _ = DockerPruneScope::Off;
    let _ = PathBuf::new();
}
