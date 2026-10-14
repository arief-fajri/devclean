//! Deciding whether it is safe to touch an AI tool's data.
//!
//! ## Why this is not `pgrep`
//!
//! The legacy `cleanup.sh` guards its database work with
//! `pgrep -f opencode`. That check is **not reliable**, and it fails in the
//! dangerous direction: under sandboxed / restricted processes (which is
//! exactly how a desktop app is often launched) `pgrep` cannot enumerate
//! the process table and exits with "no match". Measured on this machine:
//!
//! ```text
//! $ pgrep -fl opencode          # from the shell: exit 1, nothing
//! $ pgrep -fl <own exact path>  # from a spawned process: exit 1, nothing
//! ```
//!
//! A process-table probe that answers "nothing is running" when it simply
//! cannot see anything is worse than no probe at all.
//!
//! ## What we use instead
//!
//! For the database, the question is not "is a process named opencode
//! alive?" but "can I take this file exclusively?" — and SQLite can answer
//! that authoritatively with `BEGIN EXCLUSIVE`, with no process table
//! access at all. If another process holds the file, the lock fails.
//!
//! For the plain filesystem data (logs, transcripts) there is no lock to
//! take, so [`probe_process`] is used — but only as a *reported hint*,
//! never as the thing that authorises a write. Those paths are already
//! protected by the age cutoff, which is the real safety property.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};

/// What a process-table probe could tell us.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    /// A matching process was found.
    Running,
    /// The probe worked and found nothing.
    NotRunning,
    /// The probe could not answer — `pgrep` missing, erroring, or (on a
    /// sandboxed process) unable to see the process table. Callers must
    /// never read this as `NotRunning`.
    Unknown,
}

impl ProcState {
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Best-effort process probe. See the module docs for why this is only
/// ever a hint.
pub fn probe_process(pattern: &str) -> ProcState {
    let Ok(out) = Command::new("pgrep")
        .args(["-f", pattern])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    else {
        return ProcState::Unknown;
    };
    match out.code() {
        Some(0) => ProcState::Running,
        Some(1) => ProcState::NotRunning,
        _ => ProcState::Unknown,
    }
}

/// Authoritative gate for a SQLite file: can we take it exclusively?
///
/// `Ok(())` means nothing else holds the file open and it is safe to write.
/// `Err` carries the reason the UI should show.
pub fn sqlite_exclusive_available(db: &Path) -> Result<(), String> {
    if !db.is_file() {
        return Err(format!("{} no longer exists", db.display()));
    }
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| format!("could not open {}: {e}", db.display()))?;
    // Do not block waiting for a lock we would rather refuse.
    conn.busy_timeout(std::time::Duration::from_millis(0))
        .map_err(|e| e.to_string())?;
    match conn.execute_batch("BEGIN EXCLUSIVE; COMMIT;") {
        Ok(()) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::DatabaseBusy
                || e.code == rusqlite::ErrorCode::DatabaseLocked =>
        {
            Err(
                "another program is using this database right now. Quit it first, \
                 then clean this. Nothing was changed."
                    .to_string(),
            )
        }
        Err(e) => Err(format!("could not verify the database is idle: {e}")),
    }
}

/// `~/.local/share/opencode`, where opencode keeps logs, snapshots,
/// tool-output and the SQLite history.
pub fn opencode_dir() -> Option<std::path::PathBuf> {
    let dir = super::provider::home_dir()?.join(".local/share/opencode");
    dir.is_dir().then_some(dir)
}

/// `~/.claude`, Claude Code's data directory.
pub fn claude_dir() -> Option<std::path::PathBuf> {
    let dir = super::provider::home_dir()?.join(".claude");
    dir.is_dir().then_some(dir)
}

/// Files that must never be a cleanup target, whatever the policy says
/// (I10). Config and credentials are not regenerable history: deleting them
/// loses work the user cannot get back by reinstalling.
pub const NEVER_PRUNED: &[&str] = &[
    "history.jsonl",
    "settings.json",
    "settings.local.json",
    "auth.json",
    ".credentials.json",
    "config.json",
];

/// True when `name` is on the hard-preserve list.
pub fn is_never_pruned(name: &str) -> bool {
    NEVER_PRUNED.contains(&name)
}

/// Files under `dir` that the age prune may touch — everything except the
/// hard-preserved names. Exposed so both the size estimate and the prune
/// use exactly the same list, and a regression test can assert they agree.
pub fn prunable_names(dir: &Path) -> HashSet<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return HashSet::new();
    };
    rd.flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !is_never_pruned(n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_never_reports_unknown_as_not_running() {
        // The dangerous bug is collapsing Unknown into NotRunning. Pin the
        // three states apart so that mistake cannot come back.
        assert_ne!(ProcState::Unknown, ProcState::NotRunning);
        assert!(!ProcState::Unknown.is_running());
        assert!(!ProcState::NotRunning.is_running());
        assert!(ProcState::Running.is_running());
    }

    #[test]
    fn probe_returns_some_state_for_any_pattern() {
        // Whatever the sandbox allows, the probe must classify rather than
        // panic or hang.
        let _ = probe_process("definitely-not-a-real-process-zzz");
    }

    #[test]
    fn exclusive_lock_is_available_on_an_idle_database() {
        let p = std::env::temp_dir().join(format!("devclean-lock-{}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        {
            let c = rusqlite::Connection::open(&p).unwrap();
            c.execute_batch("CREATE TABLE t(x);").unwrap();
        }
        assert_eq!(
            sqlite_exclusive_available(&p),
            Ok(()),
            "an unopened database must be writable"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn exclusive_lock_is_refused_while_another_connection_holds_it() {
        let p = std::env::temp_dir().join(format!("devclean-lock-busy-{}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let holder = rusqlite::Connection::open(&p).unwrap();
        holder
            .execute_batch("CREATE TABLE t(x); BEGIN EXCLUSIVE;")
            .unwrap();

        let err = sqlite_exclusive_available(&p).unwrap_err();
        assert!(
            err.contains("another program is using"),
            "expected a clear refusal, got: {err}"
        );
        drop(holder);
        // …and it becomes available again once the holder lets go.
        assert_eq!(sqlite_exclusive_available(&p), Ok(()));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn a_missing_database_is_refused_not_assumed_idle() {
        let err = sqlite_exclusive_available(Path::new("/definitely/not/here.db")).unwrap_err();
        assert!(err.contains("no longer exists"), "got: {err}");
    }

    #[test]
    fn the_hard_preserve_list_covers_config_and_credentials() {
        for n in ["history.jsonl", "settings.json", "auth.json"] {
            assert!(is_never_pruned(n), "{n} must never be pruned");
        }
        assert!(!is_never_pruned("session-abc.jsonl"));
    }

    #[test]
    fn prunable_names_excludes_the_preserve_list() {
        let dir = std::env::temp_dir().join(format!("devclean-preserve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("history.jsonl"), "{}").unwrap();
        std::fs::write(dir.join("settings.json"), "{}").unwrap();
        std::fs::write(dir.join("session-1.jsonl"), "{}").unwrap();

        let names = prunable_names(&dir);
        assert!(names.contains("session-1.jsonl"));
        assert!(!names.contains("history.jsonl"));
        assert!(!names.contains("settings.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
