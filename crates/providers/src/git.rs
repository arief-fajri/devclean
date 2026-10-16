//! Git-aware classification (Wave B).
//!
//! Two things the filesystem alone cannot tell us:
//!
//! - Whether an artifact is **tracked** by a repository. If a project ever
//!   commits `dist/` or `target/`, deleting it as if it were a cache is data
//!   loss.
//! - Whether the repository has **uncommitted changes**. A tracked artifact
//!   in a dirty repository is the dangerous case; a gitignored one
//!   (`node_modules`) stays cleanable even when the repo is dirty, which is
//!   what keeps the tool usable in the repositories people actually work in.
//!
//! Every call is `Command::new("git").args(["-C", root, …])` with fixed
//! arguments, never a shell, and carries a deadline (G-REL-05, G-SAFE-02).
//! [`repo_root_for`] deliberately spawns nothing: `git rev-parse
//! --show-toplevel` per artifact would mean eighty subprocesses for eighty
//! `node_modules`, while walking the path's ancestors for a `.git` entry is
//! zero subprocesses and correct for the normal case.
//!
//! ## Production respects the user's global ignore
//!
//! `check-ignore` is run **without** overriding `core.excludesFile`: whether
//! a path is ignored must honour the user's own global ignore. Only test
//! fixtures pin that setting explicitly, so a runner's environment cannot
//! change the answer.

use crate::exec::{run_with_deadline, run_with_input};
use devclean_core::PROVIDER_COMMAND_TIMEOUT;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Repository root containing `path`, or `None`. **No subprocess.**
///
/// Walks `path` and its ancestors looking for a `.git` entry — a directory
/// for an ordinary clone, a *file* for a worktree or submodule — and returns
/// the first ancestor that has one. That ancestor is the repository root,
/// which is what `git -C <root>` needs.
pub fn repo_root_for(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// True when a usable `git` is on PATH. The scanner checks this once and
/// skips classification entirely when it is false, so a machine without git
/// behaves exactly as it did before Wave B.
pub fn git_available() -> bool {
    run_with_deadline("git", &["--version"], PROVIDER_COMMAND_TIMEOUT)
        .is_some_and(|o| o.status.success())
}

/// Unix seconds of `root`'s last commit, or `None` (no commits, not a repo,
/// or git could not answer).
pub fn last_commit_unix(root: &Path) -> Option<u64> {
    let out = run_git(root, &["log", "-1", "--format=%ct"])?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// True when `root` has uncommitted changes.
///
/// **Fail-safe**: when the status cannot be read the repository is treated as
/// dirty, because the alternative — treating an unreadable repository as
/// clean — is how a tracked artifact gets deleted. The scanner only reaches
/// this after confirming git is available, so a missing git is handled
/// earlier as "unclassified", not "dirty".
pub fn is_dirty(root: &Path) -> bool {
    match run_git(root, &["status", "--porcelain"]) {
        Some(out) if out.status.success() => !out.stdout.is_empty(),
        _ => true,
    }
}

/// Which of `candidates` are ignored by `root`'s ignore rules, in one call.
///
/// `git check-ignore --stdin -z` reads NUL-separated paths and echoes the
/// ignored ones. Tracked files are not reported (git does not apply ignore
/// rules to them), which is exactly the distinction the gate needs: an
/// ignored path is deletable even in a dirty repository, a tracked one is
/// not. An empty result is returned when git cannot answer.
pub fn ignored_paths(root: &Path, candidates: &[PathBuf]) -> HashSet<PathBuf> {
    if candidates.is_empty() {
        return HashSet::new();
    }
    #[cfg(test)]
    CHECK_IGNORE_CALLS.with(|c| c.set(c.get() + 1));
    let mut input = Vec::new();
    for c in candidates {
        input.extend_from_slice(c.to_string_lossy().as_bytes());
        input.push(0);
    }
    let root_s = root.to_string_lossy().into_owned();
    let Some(out) = run_with_input(
        "git",
        &["-C", &root_s, "check-ignore", "--stdin", "-z"],
        &input,
        PROVIDER_COMMAND_TIMEOUT,
    ) else {
        return HashSet::new();
    };
    out.stdout
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(String::from_utf8_lossy(s).into_owned()))
        .collect()
}

/// `git -C <root> <args>` under the provider-command deadline.
fn run_git(root: &Path, args: &[&str]) -> Option<Output> {
    let root_s = root.to_string_lossy().into_owned();
    let mut argv: Vec<&str> = vec!["-C", &root_s];
    argv.extend_from_slice(args);
    run_with_deadline("git", &argv, PROVIDER_COMMAND_TIMEOUT)
}

// Counts `ignored_paths` invocations, so the batching test can prove that N
// candidate paths in one repository cost one subprocess, not N. Thread-local
// (and test-only) so parallel tests never observe each other's calls.
#[cfg(test)]
thread_local! {
    static CHECK_IGNORE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// Skip with a clear message when git is absent, rather than failing a
    /// test for a missing external tool (G-CI-03).
    fn git_or_skip() -> bool {
        if git_available() {
            return true;
        }
        eprintln!("skipping git test: `git` is not installed");
        false
    }

    /// Run git with the fixture's own identity and config, never the
    /// machine's: a runner has no global git identity, and a user's
    /// `core.excludesFile` would change what `check-ignore` answers.
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
        let d = std::env::temp_dir().join(format!("devclean-git-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn repo_root_finds_a_git_directory_and_none_outside() {
        let root = scratch("rootdir");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("app/node_modules")).unwrap();
        assert_eq!(
            repo_root_for(&root.join("app/node_modules")),
            Some(root.clone())
        );
        let _ = std::fs::remove_dir_all(&root);

        let outside = scratch("noroot");
        std::fs::create_dir_all(outside.join("a/b/c")).unwrap();
        assert_eq!(repo_root_for(&outside.join("a/b/c")), None);
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// Worktrees and submodules mark the repo with a `.git` **file**, not a
    /// directory. Missing that would leave every worktree unclassified.
    #[test]
    fn repo_root_accepts_a_git_file_as_well_as_a_directory() {
        let root = scratch("rootfile");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".git"), "gitdir: /somewhere/else\n").unwrap();
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        assert_eq!(repo_root_for(&root.join("pkg")), Some(root.clone()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_clean_repo_is_not_dirty_and_a_dirty_one_is() {
        if !git_or_skip() {
            return;
        }
        let root = scratch("dirty");
        git(&root, &["init"]);
        std::fs::write(root.join("committed.txt"), "hi").unwrap();
        git(&root, &["add", "committed.txt"]);
        git(&root, &["commit", "-m", "initial"]);

        assert!(!is_dirty(&root), "a freshly committed repo is clean");
        assert!(last_commit_unix(&root).is_some(), "a commit must be dated");

        std::fs::write(root.join("new.txt"), "uncommitted").unwrap();
        assert!(is_dirty(&root), "an untracked file makes the repo dirty");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn check_ignore_reports_ignored_paths_only() {
        if !git_or_skip() {
            return;
        }
        let root = scratch("ignore");
        git(&root, &["init"]);
        std::fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::write(root.join("tracked.txt"), "tracked").unwrap();
        git(&root, &["add", ".gitignore", "tracked.txt"]);
        git(&root, &["commit", "-m", "initial"]);

        let ignored = ignored_paths(
            &root,
            &[
                root.join("node_modules"),
                root.join("tracked.txt"),
                root.join("not-there"),
            ],
        );
        assert!(
            ignored.contains(&root.join("node_modules")),
            "the ignored cache must be reported: {ignored:?}"
        );
        assert!(
            !ignored.contains(&root.join("tracked.txt")),
            "a tracked file is not ignored"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An empty candidate list must not spawn a process.
    #[test]
    fn check_ignore_with_no_candidates_is_empty_without_a_call() {
        let ignored = ignored_paths(Path::new("/definitely/not/a/repo"), &[]);
        assert!(ignored.is_empty());
    }

    /// Batching actually batches: every path in one repository goes through a
    /// single `check-ignore --stdin -z`, not one call per path.
    #[test]
    fn one_check_ignore_call_per_repo() {
        if !git_or_skip() {
            return;
        }
        let root = scratch("batch");
        git(&root, &["init"]);
        std::fs::write(root.join(".gitignore"), "a/\nb/\nc/\n").unwrap();
        git(&root, &["add", ".gitignore"]);
        git(&root, &["commit", "-m", "initial"]);
        // Directory patterns match existing directories only.
        for d in ["a", "b", "c"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }

        let before = CHECK_IGNORE_CALLS.with(|c| c.get());
        let ignored = ignored_paths(&root, &[root.join("a"), root.join("b"), root.join("c")]);
        let calls = CHECK_IGNORE_CALLS.with(|c| c.get()) - before;
        assert_eq!(calls, 1, "three candidates must cost one subprocess");
        assert_eq!(ignored.len(), 3, "all three are ignored");
        let _ = std::fs::remove_dir_all(&root);
    }
}
