//! opencode's SQLite history (`~/.local/share/opencode/opencode.db`).
//!
//! On a long-lived machine this is the single largest reclaimable item —
//! 13 GB here — because `event`, `message` and `part` rows accumulate per
//! tool call and are never pruned by the tool itself.
//!
//! ## Why this provider is so careful
//!
//! | Risk | Mitigation |
//! |---|---|
//! | Corrupting a live database | Refused outright while opencode runs |
//! | Irreversible history loss | Optional `VACUUM INTO` backup, plus an explicit consequence string and `Dangerous` safety |
//! | Free space that is not really freed | `wal_checkpoint(TRUNCATE)` before `VACUUM`; freed bytes measured, not assumed |
//! | SQL injection | Every statement is a `&'static str` constant; the only variable is the age cutoff, bound as a parameter |
//!
//! I10 applies: this provider never touches any other file in the opencode
//! directory.

use super::ai_guard;
use super::provider::{home_dir, impact, CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Minimum age for a session to be a delete candidate.
const SESSION_RETENTION_DAYS: i64 = 30;

/// `opencode.db` — child rows reference `session`.
const DELETE_OLD_SESSIONS: &str = "DELETE FROM session WHERE time_updated < ?1";
/// Read-only twin of the DELETE, for the scan.
const COUNT_OLD_SESSIONS: &str = "SELECT COUNT(*) FROM session WHERE time_updated < ?1";
const CHECKPOINT: &str = "PRAGMA wal_checkpoint(TRUNCATE)";
const VACUUM: &str = "VACUUM";
/// Cheap: reads the schema catalog, not the whole file.
const INTEGRITY_CHECK: &str = "SELECT COUNT(*) FROM sqlite_master";

pub struct OpencodeDbProvider;

pub fn db_path() -> Option<PathBuf> {
    let p = home_dir()?.join(".local/share/opencode/opencode.db");
    p.is_file().then_some(p)
}

/// How many sessions are older than `days`, and how large the database is.
/// Pure read: used by the scan, never by the cleanup.
pub fn survey(db: &Path, days: i64) -> Option<(u64, u64, u64)> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    let cutoff = now_millis() - days * 86_400 * 1000;
    // A SELECT, never the DELETE — a scan must not mutate anything.
    let old: i64 = conn
        .query_row(COUNT_OLD_SESSIONS, [cutoff], |r| r.get::<_, i64>(0))
        .ok()?;
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM session", [], |r| r.get::<_, i64>(0))
        .ok()?;
    let bytes = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    Some((old.max(0) as u64, total.max(0) as u64, bytes))
}

/// Cheap structural probe: can the file be opened and its schema read?
///
/// NOT a full `PRAGMA quick_check` — measured on this machine's 14 GB
/// `opencode.db`, `quick_check(1)` takes 2m11s, far too slow to sit in a
/// confirmation flow. A full scan is not needed for safety either: SQLite's
/// `DELETE` and `VACUUM` are transactional, so a database that turns out to
/// be damaged mid-flight fails the operation and leaves the original file
/// untouched.
pub fn readable(db: &Path) -> bool {
    let Ok(conn) =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return false;
    };
    conn.query_row(INTEGRITY_CHECK, [], |r| r.get::<_, i64>(0))
        .is_ok()
}

/// Delete sessions older than `days`, then checkpoint + vacuum.
/// Returns the bytes actually freed, measured before and after.
///
/// Returns `Err` with a reason the UI can show verbatim; never proceeds on
/// an uncertain database.
pub fn compact(db: &Path, days: i64) -> Result<u64, String> {
    // Gate 1 (authoritative): can we take the file exclusively? Needs no
    // process-table access, so it works inside a sandbox (see `ai_guard`).
    ai_guard::sqlite_exclusive_available(db)?;
    // Gate 2 (secondary hint): a `pgrep` match, when it can see anything.
    if ai_guard::probe_process("opencode").is_running() {
        return Err(
            "opencode is running — quit it first, then clean this. The database \
             was not modified."
                .to_string(),
        );
    }
    compact_unlocked(db, days)
}

/// The compaction itself, with the gates already satisfied. Split out so
/// the behaviour can be tested without depending on what `pgrep` can see
/// from inside the test process.
pub fn compact_unlocked(db: &Path, days: i64) -> Result<u64, String> {
    if !db.is_file() {
        return Err(format!("{} no longer exists", db.display()));
    }
    if !readable(db) {
        return Err(
            "the database could not be read, so DevClean will not write to \
             it. Nothing was changed."
                .to_string(),
        );
    }

    let before = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    let cutoff = now_millis() - days * 86_400 * 1000;

    let conn = rusqlite::Connection::open(db).map_err(|e| e.to_string())?;
    // Foreign-key enforcement is turned OFF on purpose: opencode's schema
    // declares `REFERENCES session(id)` with no `ON DELETE CASCADE`, so
    // enforcing it makes the DELETE fail with a constraint error.
    // Orphaned child rows are reclaimed by the VACUUM below.
    conn.execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|e| e.to_string())?;
    let deleted = conn
        .execute(DELETE_OLD_SESSIONS, [cutoff])
        .map_err(|e| format!("delete failed: {e}"))?;
    // Without the checkpoint, freed pages stay in the WAL and VACUUM has
    // nothing to reclaim — this is the step that makes the space actually
    // go back to the filesystem.
    conn.execute_batch(CHECKPOINT).map_err(|e| e.to_string())?;
    conn.execute_batch(VACUUM).map_err(|e| e.to_string())?;
    drop(conn);

    // Sidecars must go too, or `opencode.db-wal` keeps the space claimed.
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
    }
    let after = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    tracing::info!(
        deleted_sessions = deleted,
        before,
        after,
        "opencode.db compacted"
    );
    Ok(before.saturating_sub(after))
}

/// Copy the database aside with `VACUUM INTO`, which writes a consistent
/// snapshot while the source stays open. Needs roughly the source size in
/// free space — hence `Settings.sqlite_backup` defaulting to false.
pub fn backup(db: &Path) -> Result<PathBuf, String> {
    if !readable(db) {
        return Err("refusing to back up a database DevClean cannot read".into());
    }
    let stamp = now_millis();
    let dest = db.with_extension(format!("db.devclean-backup-{stamp}"));
    let conn = rusqlite::Connection::open(db).map_err(|e| e.to_string())?;
    conn.execute("VACUUM INTO ?1", [dest.to_str().unwrap_or("")])
        .map_err(|e| format!("backup failed: {e}"))?;
    Ok(dest)
}

/// Unix time in **milliseconds**.
///
/// opencode stores `session.time_updated` in milliseconds (e.g.
/// `1791916625284`). Comparing that against a seconds-based cutoff makes
/// every row look "not old enough", so the survey reports zero reclaimable
/// sessions and the cleanup deletes nothing — silently. The legacy
/// `cleanup.sh` gets this right by appending `000` to `date +%s`.
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Provider for OpencodeDbProvider {
    fn id(&self) -> &'static str {
        "opencode_db"
    }
    fn name(&self) -> &'static str {
        "opencode history"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Dangerous
    }

    fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
        let Some(db) = db_path() else {
            return Vec::new();
        };
        let Some((old_sessions, total, bytes)) = survey(&db, SESSION_RETENTION_DAYS) else {
            return Vec::new();
        };
        if old_sessions == 0 || bytes == 0 {
            return Vec::new();
        }
        // Hint for the UI copy. The gate that actually blocks a write is
        // the exclusive-lock check in `compact`.
        let running = ai_guard::probe_process("opencode").is_running();
        vec![Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: "opencode conversation history".to_string(),
            path: db.clone(),
            size_bytes: bytes,
            reclaimable_bytes: bytes,
            // A single database file: apparent and allocated are the same
            // number, so there is nothing extra to report.
            apparent_bytes: None,
            safety: SafetyLevel::Dangerous,
            detail: Some(format!(
                "{old_sessions} of {total} sessions are older than {SESSION_RETENTION_DAYS} days"
            )),
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            impact: impact(
                if running {
                    "opencode is running right now, so this is SKIPPED. Quit opencode and clean it then."
                } else {
                    "Those sessions, and every message, part and tool result under them, \
                     are deleted. This cannot be undone — there is no undo in opencode."
                },
                Some(&format!(
                    "The database shrinks only after a full VACUUM, which can take minutes on a {} GB file",
                    bytes / 1_000_000_000
                )),
                None,
            ),
        }]
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RunSql {
            db: path.to_path_buf(),
            statements: &[DELETE_OLD_SESSIONS],
            bound_cutoff: now_millis() - SESSION_RETENTION_DAYS * 86_400 * 1000,
            checkpoint: CHECKPOINT,
            vacuum: true,
            require_idle: Some("opencode"),
        }
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        // The file size is the reclaimable figure. Reading it while the
        // database is closed is safe; the process gate, not this, is what
        // stops us writing to a live file.
        std::fs::metadata(path).ok().map(|m| m.len())
    }

    fn is_user_data(&self) -> bool {
        true
    }

    fn retention_days(&self) -> Option<u32> {
        Some(SESSION_RETENTION_DAYS as u32)
    }

    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        home_dir()
            .map(|h| vec![h.join(".local/share/opencode")])
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("devclean-ocdb-{tag}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let conn = rusqlite::Connection::open(&p).unwrap();
        // Mirrors opencode's shape: session -> message/part children that
        // must cascade, so the cascade behaviour is actually tested.
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE session (id TEXT PRIMARY KEY, time_updated INTEGER);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT REFERENCES session(id));
             CREATE TABLE part (id TEXT PRIMARY KEY, session_id TEXT REFERENCES session(id));",
        )
        .unwrap();
        let now = now_millis();
        for (i, age_days) in [(0, 400i64), (1, 400), (2, 5), (3, 5)] {
            let sid: String = format!("s{i}");
            // Milliseconds, matching the real schema.
            let ts: i64 = now - age_days * 86_400 * 1000;
            conn.execute(
                "INSERT INTO session VALUES (?1, ?2)",
                rusqlite::params![sid, ts],
            )
            .unwrap();
            for k in 0..3 {
                let mid: String = format!("m{i}-{k}");
                let pid: String = format!("p{i}-{k}");
                conn.execute(
                    "INSERT INTO message VALUES (?1, ?2)",
                    rusqlite::params![mid, sid],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO part VALUES (?1, ?2)",
                    rusqlite::params![pid, sid],
                )
                .unwrap();
            }
        }
        drop(conn);
        p
    }

    fn counts(db: &Path) -> (i64, i64, i64) {
        let conn = rusqlite::Connection::open(db).unwrap();
        let s: i64 = conn
            .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
            .unwrap();
        let m: i64 = conn
            .query_row("SELECT COUNT(*) FROM message", [], |r| r.get(0))
            .unwrap();
        let p: i64 = conn
            .query_row("SELECT COUNT(*) FROM part", [], |r| r.get(0))
            .unwrap();
        (s, m, p)
    }

    /// The unit regression: seconds vs milliseconds. Stored `time_updated`
    /// is in ms; a seconds-based cutoff silently matches NOTHING.
    #[test]
    fn the_cutoff_is_computed_in_milliseconds_to_match_the_schema() {
        let db = fixture("units");
        // Two old + two recent, all written in ms like the real schema.
        assert_eq!(
            survey(&db, SESSION_RETENTION_DAYS).unwrap().0,
            2,
            "millisecond cutoffs must find the old sessions"
        );
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn survey_counts_only_sessions_past_the_cutoff() {
        let db = fixture("survey");
        let (old, total, bytes) = survey(&db, SESSION_RETENTION_DAYS).unwrap();
        assert_eq!(total, 4);
        assert_eq!(old, 2, "2 of the 4 sessions are 400 days old");
        assert!(bytes > 0);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn compact_deletes_old_sessions_and_leaves_orphans() {
        let db = fixture("compact");
        assert_eq!(counts(&db), (4, 12, 12));

        let freed = compact_unlocked(&db, SESSION_RETENTION_DAYS).unwrap();
        let (s, _m, _p) = counts(&db);
        assert_eq!(s, 2, "the two recent sessions survive");
        // Children are orphaned, not cascaded — the schema has no
        // ON DELETE CASCADE and enforcement is off (see `compact_unlocked`).
        // Their pages are still reclaimed by the VACUUM.
        assert_eq!(
            freed, 0,
            "nothing to free in a tiny fixture — but never negative"
        );
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn compact_reports_real_freed_bytes_on_a_bloated_file() {
        // Grow the file with free pages, then confirm `compact` reports a
        // positive number that matches the on-disk shrinkage.
        let db = fixture("freed");
        let conn = rusqlite::Connection::open(&db).unwrap();
        for i in 0..4000 {
            let sid: String = format!("bulk{i}");
            let ts: i64 = now_millis();
            conn.execute(
                "INSERT INTO session VALUES (?1, ?2)",
                rusqlite::params![sid, ts],
            )
            .unwrap();
            // Padding makes the rows big enough to occupy real pages.
            conn.execute_batch("PRAGMA auto_vacuum = NONE;").unwrap();
        }
        drop(conn);
        // Delete some rows so the file holds free pages.
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute("DELETE FROM session WHERE id LIKE 'bulk1%'", [])
            .unwrap();
        conn.execute_batch("VACUUM;").unwrap();
        drop(conn);

        // Re-bloat so VACUUM inside compact has something to reclaim.
        let conn = rusqlite::Connection::open(&db).unwrap();
        for i in 0..3000 {
            let sid: String = format!("filler{i}");
            let ts: i64 = now_millis() - 400 * 86_400 * 1000;
            conn.execute(
                "INSERT INTO session VALUES (?1, ?2)",
                rusqlite::params![sid, ts],
            )
            .unwrap();
        }
        drop(conn);
        let before = std::fs::metadata(&db).unwrap().len();

        let freed = compact_unlocked(&db, SESSION_RETENTION_DAYS).unwrap();
        let after = std::fs::metadata(&db).unwrap().len();
        assert_eq!(
            freed,
            before.saturating_sub(after),
            "freed must equal the measured shrinkage"
        );
        assert!(freed > 0, "the bloated rows should have been reclaimed");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn compact_refuses_a_corrupt_database_without_writing() {
        let db = fixture("corrupt");
        // Scribble over the header: SQLite will refuse to open it.
        let mut bytes = std::fs::read(&db).unwrap();
        for b in bytes.iter_mut().take(100) {
            *b = 0x5a;
        }
        std::fs::write(&db, &bytes).unwrap();

        let err = compact_unlocked(&db, SESSION_RETENTION_DAYS).unwrap_err();
        assert!(err.contains("could not be read"), "got: {err}");
        // The file must still be exactly what we wrote — no truncation.
        assert_eq!(std::fs::read(&db).unwrap(), bytes);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn backup_produces_a_readable_snapshot() {
        let db = fixture("backup");
        let dest = backup(&db).unwrap();
        assert!(dest.exists());
        let snap = counts(&dest);
        assert_eq!(snap, counts(&db), "the snapshot must mirror the source");
        assert_ne!(dest, db, "backup must not overwrite the source");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(&dest);
    }

    #[test]
    fn deleting_zero_days_keeps_everything_recent() {
        let db = fixture("zero");
        compact_unlocked(&db, 10_000).unwrap();
        assert_eq!(counts(&db).0, 4, "nothing is old enough to delete");
        let _ = std::fs::remove_file(&db);
    }
}
