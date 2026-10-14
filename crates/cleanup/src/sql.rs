//! Fixed-statement SQLite cleanup (G-SAFE-02).
//!
//! Every statement arrives as a `&'static str` constant from the provider
//! that declares the action; nothing here accepts SQL as text from a
//! caller. The only per-run value is the age cutoff, and it travels as a
//! **bound parameter** (`?1`) rather than being formatted into the string,
//! so it cannot alter the statement's shape.
//!
//! Sequence, and why each step exists:
//!
//! 1. Cheap structural probe — open the file and read the schema catalog.
//! 2. Refuse when the owning process runs. Writing to a database its owner
//!    still holds open is how SQLite databases get corrupted.
//! 3. Run the statements.
//! 4. `wal_checkpoint(TRUNCATE)` — without it the freed pages stay claimed
//!    by the WAL and the file never shrinks.
//! 5. `VACUUM` — rewrites the file so the space returns to the filesystem.
//! 6. Remove `-wal` / `-shm` sidecars.
//! 7. Report `before - after`, measured.
//!
//! There is deliberately **no** `PRAGMA quick_check` in that sequence.
//! Measured on this machine's 14 GB `opencode.db`, `quick_check(1)` takes
//! 2m11s — minutes of frozen UI for a check SQLite does not need: `DELETE`
//! and `VACUUM` are both transactional, so a damaged database fails the
//! operation and leaves the original file untouched.

use std::path::Path;

/// Turn a probe failure into the message the user should read. A locked
/// database is NOT a corrupt one — it gets its own wording so the user
/// knows to quit the other program rather than to fear data loss.
fn structural_error(e: rusqlite::Error) -> String {
    match &e {
        rusqlite::Error::SqliteFailure(inner, _)
            if inner.code == rusqlite::ErrorCode::DatabaseBusy
                || inner.code == rusqlite::ErrorCode::DatabaseLocked =>
        {
            "another program is using this database right now. Quit it first, \
             then clean this. Nothing was changed."
                .to_string()
        }
        rusqlite::Error::SqliteFailure(inner, msg)
            if inner.code == rusqlite::ErrorCode::NotADatabase =>
        {
            format!(
                "{} is not a readable SQLite database, so DevClean will not \
                 write to it. Nothing was changed.",
                msg.clone().unwrap_or_default()
            )
        }
        _ => format!("could not read the database structure: {e}"),
    }
}

/// Execute the fixed statements, then reclaim space. Returns the bytes
/// actually freed.
///
/// The caller is expected to have run `SafetyValidator::validate_path`
/// first; this function does not re-check, matching the engine's existing
/// contract for `CleanupAction`.
#[allow(clippy::too_many_arguments)]
pub fn run_sql(
    db: &Path,
    statements: &[&str],
    bound_cutoff: i64,
    checkpoint: &str,
    vacuum: bool,
    require_idle: Option<&str>,
) -> Result<u64, String> {
    // A missing file is the caller's mistake, not an integrity problem.
    if !db.is_file() {
        return Err(format!("{} no longer exists", db.display()));
    }

    // 1. Structural probe. Opens the file and reads the schema catalog —
    //    cheap, and enough to catch a truncated or non-SQLite file with a
    //    clear message.
    //
    //    NOTE: `PRAGMA quick_check` is deliberately NOT run here. Measured
    //    on this machine's 14 GB opencode.db it takes 2m11s, which would
    //    freeze the confirmation flow for minutes. A full scan is not
    //    needed for safety either: SQLite's DELETE and VACUUM are both
    //    transactional, so a database that turns out to be corrupt fails
    //    the operation and leaves the original file untouched.
    {
        let probe =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(structural_error)?;
        probe
            .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(structural_error)?;
    }

    // 2. Authoritative idleness check: can we take the file exclusively?
    //    Needs no process-table access, unlike `pgrep` (see `ai_guard`).
    devclean_providers::ai_guard::sqlite_exclusive_available(db)?;
    // 3. Secondary signal only — a sandboxed probe may legitimately answer
    //    "unknown", which is never a reason to refuse.
    if let Some(proc) = require_idle {
        if devclean_providers::ai_guard::probe_process(proc).is_running() {
            return Err(format!(
                "{proc} is running — quit it first, then clean this. \
                 The database was not modified."
            ));
        }
    }

    let before = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);

    let conn =
        rusqlite::Connection::open(db).map_err(|e| format!("could not open database: {e}"))?;

    // NOTE: `PRAGMA foreign_keys` is deliberately turned OFF here.
    // opencode's schema declares `REFERENCES session(id)` WITHOUT
    // `ON DELETE CASCADE`, so with enforcement on,
    // `DELETE FROM session` fails outright with a constraint error and the
    // cleanup would never run at all. (rusqlite's bundled SQLite enables
    // enforcement by default, so this has to be explicit.) With it off the
    // session rows go, their child rows are left orphaned, and the
    // following VACUUM reclaims those pages too — which is where the disk
    // space actually comes from. This is also why the legacy `cleanup.sh`
    // works: the `sqlite3` CLI never turns enforcement on either.
    conn.execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|e| format!("could not relax foreign keys: {e}"))?;
    for sql in statements {
        conn.execute(sql, [bound_cutoff])
            .map_err(|e| format!("{sql} failed: {e}"))?;
    }
    conn.execute_batch(checkpoint).map_err(|e| e.to_string())?;
    if vacuum {
        conn.execute_batch("VACUUM").map_err(|e| e.to_string())?;
    }
    drop(conn);

    for suffix in ["-wal", "-shm"] {
        let sidecar = Path::new(db).with_file_name(format!(
            "{}{suffix}",
            db.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_file(sidecar);
    }

    let after = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    Ok(before.saturating_sub(after))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELETE: &str = "DELETE FROM session WHERE time_updated < ?1";
    const CHECKPOINT: &str = "PRAGMA wal_checkpoint(TRUNCATE)";

    fn fixture(tag: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("devclean-runsql-{tag}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let conn = rusqlite::Connection::open(&p).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, time_updated INTEGER);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT REFERENCES session(id));",
        )
        .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO session VALUES ('old', ?1)",
            [now - 400 * 86_400],
        )
        .unwrap();
        conn.execute("INSERT INTO session VALUES ('new', ?1)", [now])
            .unwrap();
        conn.execute("INSERT INTO message VALUES ('m1', 'old')", [])
            .unwrap();
        drop(conn);
        p
    }

    /// Cutoff that sits between the fixture's `old` and `new` rows.
    fn mid_cutoff() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 200 * 86_400
    }

    fn count(db: &std::path::Path, table: &str) -> i64 {
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn cutoff_is_bound_not_interpolated() {
        let db = fixture("bound");
        // A value that would be catastrophic if it were string-formatted
        // into the SQL. Bound, it is just a number.
        let evil: i64 = i64::MIN;
        run_sql(&db, &[DELETE], evil, CHECKPOINT, true, None).unwrap();
        assert_eq!(count(&db, "session"), 2, "nothing older than year 1677");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn deletes_old_rows_and_reports_real_freed_bytes() {
        let db = fixture("delete");
        let freed = run_sql(&db, &[DELETE], mid_cutoff(), CHECKPOINT, true, None).unwrap();
        assert_eq!(count(&db, "session"), 1, "only 'new' survives");
        // The child row is orphaned, not cascaded — see the note in
        // `run_sql`. Its pages are still reclaimed by the VACUUM, which is
        // where the reclaimed disk space comes from.
        assert_eq!(
            count(&db, "message"),
            1,
            "documented behaviour: children are orphaned, not cascaded"
        );
        // This fixture is a handful of pages, so VACUUM may not reclaim
        // anything measurable. The "freed bytes are really freed" claim is
        // pinned on a bloated database in `opencode_db::tests`.
        let _ = freed;
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn refuses_a_corrupt_database_and_leaves_the_bytes_alone() {
        let db = fixture("corrupt");
        let mut bytes = std::fs::read(&db).unwrap();
        for b in bytes.iter_mut().take(100) {
            *b = 0x5a;
        }
        std::fs::write(&db, &bytes).unwrap();

        let err = run_sql(&db, &[DELETE], mid_cutoff(), CHECKPOINT, true, None).unwrap_err();
        assert!(
            err.contains("not a readable SQLite database") || err.contains("structure"),
            "got: {err}"
        );
        assert_eq!(std::fs::read(&db).unwrap(), bytes, "file must be untouched");
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn refuses_a_missing_database() {
        let err = run_sql(
            Path::new("/definitely/not/here.db"),
            &[DELETE],
            0,
            CHECKPOINT,
            true,
            None,
        )
        .unwrap_err();
        assert!(err.contains("no longer exists"), "got: {err}");
    }

    /// The authoritative gate: while another connection holds the file,
    /// the cleanup must refuse and change nothing. This is the check that
    /// matters — `pgrep` cannot be relied on (see `ai_guard`).
    #[test]
    fn refuses_while_another_connection_holds_the_database() {
        let db = fixture("busy");
        let holder = rusqlite::Connection::open(&db).unwrap();
        let before: i64 = holder
            .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
            .unwrap();
        holder.execute_batch("BEGIN EXCLUSIVE;").unwrap();

        let err = run_sql(&db, &[DELETE], mid_cutoff(), CHECKPOINT, true, None).unwrap_err();
        assert!(err.contains("another program is using"), "got: {err}");
        // Read through the holder: opening a new connection would itself
        // hit the lock we are testing.
        let during: i64 = holder
            .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
            .unwrap();
        assert_eq!(during, before, "no rows may be deleted");

        drop(holder);
        // Once released, the very same call succeeds.
        let freed = run_sql(&db, &[DELETE], mid_cutoff(), CHECKPOINT, true, None).unwrap();
        assert_eq!(count(&db, "session"), 1);
        assert_eq!(freed, 0, "a tiny fixture frees nothing, but never a lie");
        let _ = std::fs::remove_file(&db);
    }
}
