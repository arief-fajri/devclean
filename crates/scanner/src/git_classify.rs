//! Git-aware classification pass (Wave B).
//!
//! Runs once per scan, after providers have returned their resources and
//! before `blocked_reason` is stamped. It groups resources by the repository
//! that contains them and asks git three questions per repository:
//!
//! ```text
//! 1. group resources by repository root        no subprocess
//! 2. per repository, pool of min(NumCPU, 8):
//!      git log -1 --format=%ct     → activity
//!      git status --porcelain      → dirty?
//! 3. only the dirty ones: one `check-ignore --stdin -z` for all their paths
//! 4. stamp activity, last_commit_unix, tracked_in_dirty_repo
//! ```
//!
//! Step 3 is why this is affordable: a clean repository cannot produce
//! `Protected`, so it never needs `check-ignore`. Stamping is applied
//! **serially** after the workers join, so results do not depend on worker
//! completion order.
//!
//! A machine without git, or a resource outside any repository, is left
//! exactly as it was before Wave B: `activity` stays `None`, `age_days` keeps
//! whatever the provider computed, and nothing is protected.

use devclean_core::{composite_age_days, Activity, Resource};
use devclean_providers::{
    git_available, ignored_paths, is_dirty, last_commit_unix, repo_root_for, Provider,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Cap on concurrent git subprocesses, as the roadmap specifies
/// `min(NumCPU, 8)`. More is not free: each `git status` on a large tree is
/// its own process and its own page-cache pressure.
const MAX_GIT_WORKERS: usize = 8;

/// One resource inside a repository, with the project directory whose age
/// contributes to its composite.
struct Member {
    index: usize,
    project: PathBuf,
}

/// One repository and the resources it contains.
struct Job {
    root: PathBuf,
    members: Vec<Member>,
}

/// What the git pass learned about one resource.
#[derive(Clone, Copy)]
struct Classification {
    activity: Option<Activity>,
    last_commit: Option<u64>,
    age_days: Option<u64>,
    protected: bool,
}

/// Classify every resource that belongs to a git repository, mutating its
/// `activity`, `last_commit_unix` and (composite) `age_days`. Returns the ids
/// that must be refused because they are tracked by — or untracked in — a
/// repository with uncommitted changes.
///
/// `cancel` is checked before each repository is picked up, so a cancelled
/// scan stops scheduling new git work promptly (G-SCAN-02).
pub fn classify_repositories(
    resources: &mut [Resource],
    providers: &HashMap<&str, &dyn Provider>,
    cancel: &AtomicBool,
) -> HashSet<String> {
    // Group first — this is pure path walking, no subprocess. When nothing
    // lives in a repository we never even probe for git.
    let mut groups: HashMap<PathBuf, Vec<Member>> = HashMap::new();
    for (i, r) in resources.iter().enumerate() {
        let Some(p) = providers.get(r.provider.as_str()) else {
            continue;
        };
        let Some(project) = p.project_root(&r.path) else {
            continue;
        };
        let Some(root) = repo_root_for(&r.path) else {
            continue;
        };
        groups
            .entry(root)
            .or_default()
            .push(Member { index: i, project });
    }
    if groups.is_empty() || !git_available() {
        return HashSet::new();
    }

    let jobs: Vec<Job> = groups
        .into_iter()
        .map(|(root, members)| Job { root, members })
        .collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Classification)>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(MAX_GIT_WORKERS)
        .min(jobs.len())
        .max(1);
    let now = now_unix();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs.get(index) else { break };
                let batch = classify_one(job, resources, now);
                // A poisoned lock must not abort the pass: losing every
                // resource because one worker panicked would be worse than a
                // missing classification.
                if let Ok(mut results) = results.lock() {
                    results.extend(batch);
                }
            });
        }
    });

    let mut protected = HashSet::new();
    for (i, c) in results.into_inner().unwrap_or_default() {
        let r = &mut resources[i];
        if c.age_days.is_some() {
            r.age_days = c.age_days;
        }
        r.activity = c.activity;
        r.last_commit_unix = c.last_commit;
        if c.protected {
            protected.insert(r.id.clone());
        }
    }
    protected
}

/// Ask git about one repository and classify each of its resources. Pure with
/// respect to `resources` (only reads paths/mtimes), so it is safe to call
/// from a worker thread.
fn classify_one(job: &Job, resources: &[Resource], now: u64) -> Vec<(usize, Classification)> {
    let last_commit = last_commit_unix(&job.root);
    let dirty = is_dirty(&job.root);
    let ignored = if dirty {
        let candidates: Vec<PathBuf> = job
            .members
            .iter()
            .map(|m| resources[m.index].path.clone())
            .collect();
        ignored_paths(&job.root, &candidates)
    } else {
        HashSet::new()
    };

    job.members
        .iter()
        .map(|m| {
            let artifact_mtime = mtime_unix(&resources[m.index].path);
            let project_mtime = mtime_unix(&m.project);
            let age_days = composite_age_days(now, artifact_mtime, last_commit, project_mtime);
            let protected = dirty && !ignored.contains(&resources[m.index].path);
            (
                m.index,
                Classification {
                    activity: age_days.map(Activity::for_age_days),
                    last_commit,
                    age_days,
                    protected,
                },
            )
        })
        .collect()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn mtime_unix(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}
