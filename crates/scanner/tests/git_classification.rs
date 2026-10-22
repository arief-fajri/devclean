//! Evidence for Wave B: git-aware classification and the `Protected` gate.
//!
//! Every fixture states its own git identity and configuration — a runner has
//! no global git identity, and a user's `core.excludesFile` would otherwise
//! change what `check-ignore` answers (G-CI-03). A missing `git` skips the
//! test with a message rather than failing for an absent external tool.

use devclean_core::{Activity, BlockReason, PolicyInputs, Resource, SafetyLevel, ScanSummary};
use devclean_providers::{
    git_available, repo_root_for, CleanupAction, OwnScanner, Provider, Scannable,
};
use devclean_scanner::ScanManager;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// A provider whose resource is a directory that belongs to a project, so
/// the scanner's git pass classifies it.
struct ProjectStub {
    artifact: PathBuf,
    project: PathBuf,
}

impl Provider for ProjectStub {
    fn id(&self) -> &'static str {
        "project_stub"
    }
    fn name(&self) -> &'static str {
        "Project stub"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Safe
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        devclean_providers::measure_dir(path)
    }
    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }
    fn project_root(&self, _artifact: &Path) -> Option<PathBuf> {
        Some(self.project.clone())
    }
}

async fn scan(stub: ProjectStub) -> ScanSummary {
    let mgr = ScanManager::with_config(
        vec![Scannable::Own(Arc::new(stub))],
        Duration::from_secs(10),
    );
    mgr.scan_all(vec![], &[], &[], PolicyInputs::default(), None, None)
        .await
}

fn git_or_skip() -> bool {
    if git_available() {
        return true;
    }
    eprintln!("skipping git test: `git` is not installed");
    false
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "core.excludesFile=/dev/null",
            "-c",
            "core.fileMode=false",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git spawn");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("devclean-gitc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn set_mtime_days_ago(path: &Path, days: u64) {
    let when = SystemTime::now() - Duration::from_secs(days * 86_400);
    std::fs::File::open(path)
        .expect("open for mtime")
        .set_modified(when)
        .expect("set mtime");
}

/// A tracked artifact in a repository with uncommitted changes is the
/// dangerous case: deleting it loses work that is not committed anywhere.
#[tokio::test]
async fn a_tracked_artifact_in_a_dirty_repo_is_protected() {
    if !git_or_skip() {
        return;
    }
    let repo = scratch("tracked-dirty");
    git(&repo, &["init"]);
    std::fs::create_dir_all(repo.join("dist")).unwrap();
    std::fs::write(repo.join("dist/app.js"), "built").unwrap();
    git(&repo, &["add", "dist"]);
    git(&repo, &["commit", "-m", "track dist"]);
    // Dirty the repo with an unrelated uncommitted file.
    std::fs::write(repo.join("work-in-progress.txt"), "not committed").unwrap();

    let s = scan(ProjectStub {
        artifact: repo.join("dist"),
        project: repo.clone(),
    })
    .await;
    assert_eq!(
        s.resources[0].blocked_reason,
        Some(BlockReason::Protected),
        "a tracked artifact in a dirty repo must be refused"
    );
    assert!(s.blocked_bytes > 0, "held-back bytes must be reported");
    let _ = std::fs::remove_dir_all(&repo);
}

/// The row that keeps this from being useless: a gitignored cache stays
/// cleanable even when the repository is dirty.
#[tokio::test]
async fn a_gitignored_artifact_stays_cleanable_in_a_dirty_repo() {
    if !git_or_skip() {
        return;
    }
    let repo = scratch("ignored-dirty");
    git(&repo, &["init"]);
    std::fs::write(repo.join(".gitignore"), "node_modules/\n").unwrap();
    git(&repo, &["add", ".gitignore"]);
    git(&repo, &["commit", "-m", "ignore node_modules"]);
    std::fs::create_dir_all(repo.join("node_modules/pkg")).unwrap();
    std::fs::write(repo.join("node_modules/pkg/index.js"), "dep").unwrap();
    std::fs::write(repo.join("work-in-progress.txt"), "not committed").unwrap();

    let s = scan(ProjectStub {
        artifact: repo.join("node_modules"),
        project: repo.clone(),
    })
    .await;
    assert_eq!(
        s.resources[0].blocked_reason, None,
        "one dirty file must not block node_modules"
    );
    assert_eq!(s.blocked_bytes, 0);
    let _ = std::fs::remove_dir_all(&repo);
}

/// No false positives: a clean repository protects nothing.
#[tokio::test]
async fn a_clean_repo_protects_nothing() {
    if !git_or_skip() {
        return;
    }
    let repo = scratch("clean");
    git(&repo, &["init"]);
    std::fs::create_dir_all(repo.join("dist")).unwrap();
    std::fs::write(repo.join("dist/app.js"), "built").unwrap();
    git(&repo, &["add", "dist"]);
    git(&repo, &["commit", "-m", "track dist"]);

    let s = scan(ProjectStub {
        artifact: repo.join("dist"),
        project: repo.clone(),
    })
    .await;
    assert_eq!(s.resources[0].blocked_reason, None);
    assert_eq!(s.blocked_bytes, 0);
    assert!(
        s.resources[0].activity.is_some(),
        "a repository resource is classified even when clean"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

/// A new, untracked, non-ignored file is still a file: in a dirty repository
/// it is protected too.
#[tokio::test]
async fn an_untracked_file_in_a_dirty_repo_is_protected() {
    if !git_or_skip() {
        return;
    }
    let repo = scratch("untracked-dirty");
    git(&repo, &["init"]);
    std::fs::write(repo.join("README.md"), "hi").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-m", "initial"]);
    // The artifact itself is untracked, which makes the repo dirty.
    std::fs::create_dir_all(repo.join("cache")).unwrap();
    std::fs::write(repo.join("cache/blob"), "scratch").unwrap();

    let s = scan(ProjectStub {
        artifact: repo.join("cache"),
        project: repo.clone(),
    })
    .await;
    assert_eq!(
        s.resources[0].blocked_reason,
        Some(BlockReason::Protected),
        "an untracked file in a dirty repo counts"
    );
    let _ = std::fs::remove_dir_all(&repo);
}

/// Composite age, not mtime alone: a repository committed to recently stops
/// looking dormant even when the artifact and project directory are old.
#[tokio::test]
async fn last_commit_beats_a_stale_mtime() {
    if !git_or_skip() {
        return;
    }
    let repo = scratch("commit-beats-mtime");
    git(&repo, &["init"]);
    std::fs::create_dir_all(repo.join("dist")).unwrap();
    std::fs::write(repo.join("dist/app.js"), "built").unwrap();
    git(&repo, &["add", "dist"]);
    git(&repo, &["commit", "-m", "track dist"]);

    // The artifact and its project directory look 200 days old; only the
    // commit is recent. Without the composite the cache would read dormant.
    set_mtime_days_ago(&repo.join("dist"), 200);
    set_mtime_days_ago(&repo, 200);

    let s = scan(ProjectStub {
        artifact: repo.join("dist"),
        project: repo.clone(),
    })
    .await;
    let r = &s.resources[0];
    assert!(
        r.age_days.is_some_and(|d| d < 7),
        "the recent commit must dominate the stale mtime, got {:?}",
        r.age_days
    );
    assert_eq!(r.activity, Some(Activity::Active));
    assert!(r.last_commit_unix.is_some(), "the commit time is reported");
    let _ = std::fs::remove_dir_all(&repo);
}

/// The four activity tiers, at exactly 7 / 30 / 90 days.
#[tokio::test]
async fn activity_thresholds_at_the_boundaries() {
    if !git_or_skip() {
        return;
    }
    let repo = scratch("activity-bands");
    git(&repo, &["init"]);
    std::fs::create_dir_all(repo.join("dist")).unwrap();
    std::fs::write(repo.join("dist/app.js"), "built").unwrap();

    for (days, want) in [
        (6u64, Activity::Active),
        (7, Activity::Recent),
        (29, Activity::Recent),
        (30, Activity::Stale),
        (89, Activity::Stale),
        (90, Activity::Dormant),
    ] {
        set_mtime_days_ago(&repo.join("dist"), days);
        set_mtime_days_ago(&repo, days);
        let s = scan(ProjectStub {
            artifact: repo.join("dist"),
            project: repo.clone(),
        })
        .await;
        assert_eq!(s.resources[0].activity, Some(want), "{days}d");
    }
    let _ = std::fs::remove_dir_all(&repo);
}

/// Global caches and paths outside any repository are left unclassified:
/// `activity` stays `None` and nothing is protected.
#[tokio::test]
async fn a_path_outside_any_repo_is_not_classified() {
    let root = scratch("outside");
    if repo_root_for(&root).is_some() {
        eprintln!("skipping: the temp directory is unexpectedly inside a repository");
        return;
    }
    let project = root.join("proj");
    std::fs::create_dir_all(project.join("cache")).unwrap();
    std::fs::write(project.join("cache/blob"), "x").unwrap();

    let s = scan(ProjectStub {
        artifact: project.join("cache"),
        project,
    })
    .await;
    let r = &s.resources[0];
    assert_eq!(r.activity, None, "outside a repo there is no activity tier");
    assert_eq!(r.last_commit_unix, None);
    assert_eq!(r.blocked_reason, None);
    let _ = std::fs::remove_dir_all(&root);
}

impl OwnScanner for ProjectStub {
    fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
        vec![Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: "proj/cache".to_string(),
            path: self.artifact.clone(),
            size_bytes: 1024,
            reclaimable_bytes: 1024,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        }]
    }
}
