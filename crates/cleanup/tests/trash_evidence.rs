//! End-to-end proof that the default delete mode is recoverable: a real
//! artifact moved to the REAL `~/.Trash`, then restored by hand.
use devclean_cleanup::trash::{default_trash_dir, move_to_trash};

#[test]
fn evidence_a_real_artifact_survives_in_the_real_trash() {
    let trash = default_trash_dir().expect("HOME");
    let base = std::env::temp_dir().join(format!("devclean-evidence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let artifact = base.join("my-app/node_modules");
    std::fs::create_dir_all(artifact.join("left-pad")).unwrap();
    std::fs::write(artifact.join("left-pad/index.js"), "x".repeat(4096)).unwrap();
    let before = devclean_providers::dir_size(&artifact);

    let landed = move_to_trash(&artifact, &trash).expect("move to real Trash");

    println!("\n  original path exists : {}", artifact.exists());
    println!("  landed in Trash      : {}", landed.display());
    println!(
        "  bytes still on disk  : {} (deleting did NOT free them)",
        devclean_providers::dir_size(&landed)
    );
    assert!(!artifact.exists(), "the original path must be free");
    assert!(landed.starts_with(&trash));
    assert_eq!(
        devclean_providers::dir_size(&landed),
        before,
        "contents intact"
    );

    // The user can restore it — that is the whole point.
    let restored = base.join("restored");
    std::fs::rename(&landed, &restored).expect("restore from Trash");
    assert!(restored.join("left-pad/index.js").is_file());
    println!("  restored by hand     : OK");

    let _ = std::fs::remove_dir_all(&base);
}
