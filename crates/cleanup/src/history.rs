//! Cleanup history: append-only JSONL log (one [`HistoryEntry`] per line).
//!
//! Deliberately not SQLite: no new dependency, no migrations, and the log
//! is human-inspectable. Corrupt lines are skipped on read, never fatal.
//!
//! Reads are **tail-first** ([`read_history_runs`]): the log is never
//! truncated, so loading it head-first would grow without bound and a
//! single large cleanup would push every earlier run off the end.

use devclean_core::{CleanupRun, CleanupStatus, HistoryEntry};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Append one entry. Creates parent directories on demand.
pub fn append_history_entry(path: &Path, entry: &HistoryEntry) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", serde_json::to_string(entry).unwrap())?;
    Ok(())
}

/// Read the newest `limit` entries (oldest first). Missing file → empty;
/// corrupt lines are skipped.
///
/// Prefer [`read_history_runs`] for any UI: this entry-level limit lets one
/// big cleanup (hundreds of resources) hide every run before it.
pub fn read_history(path: &Path, limit: usize) -> Vec<HistoryEntry> {
    let entries = read_all(path);
    let skip = entries.len().saturating_sub(limit.max(1));
    entries.into_iter().skip(skip).collect()
}

/// Read the newest `max_runs` cleanup runs, newest run first, with every
/// per-resource entry attached. Reads the file's tail only, so cost is
/// proportional to the runs requested, not to the log's total size.
///
/// Legacy entries (written before `run_id` existed) carry an empty id and
/// are grouped by `finished_at_unix`, so old history stays visible.
pub fn read_history_runs(path: &Path, max_runs: usize) -> Vec<CleanupRun> {
    let max_runs = max_runs.max(1);
    let mut entries = read_tail_lines(path, max_runs);
    // `entries` is newest-first; reverse so each run aggregates in
    // chronological order and the final sort is stable.
    entries.reverse();
    if entries.is_empty() {
        return Vec::new();
    }

    let mut runs: Vec<CleanupRun> = Vec::new();
    for e in entries {
        match runs.last_mut() {
            // Same run → append.
            Some(last) if same_run(last, &e) => push_entry(last, e),
            // New run → start one.
            _ => {
                let mut run = empty_run(&e);
                push_entry(&mut run, e);
                runs.push(run);
            }
        }
    }
    // Runs were built oldest-first; the UI wants the newest first and only
    // `max_runs` of them. `read_tail_lines` already bounded the input to
    // roughly max_runs + 1, so nothing large is accumulated.
    runs.reverse();
    runs.truncate(max_runs);
    runs
}

/// Run identity: the explicit id, or the timestamp for legacy lines.
fn run_key(e: &HistoryEntry) -> String {
    if e.run_id.is_empty() {
        format!("legacy-{}", e.finished_at_unix)
    } else {
        e.run_id.clone()
    }
}

/// Whether `next` belongs to `run`. Runs are built in chronological order,
/// so the first entry identifies the run.
fn same_run(run: &CleanupRun, next: &HistoryEntry) -> bool {
    run.entries
        .first()
        .is_some_and(|first| run_key(first) == run_key(next))
}

fn empty_run(e: &HistoryEntry) -> CleanupRun {
    CleanupRun {
        run_id: e.run_id.clone(),
        finished_at_unix: e.finished_at_unix,
        entry_count: 0,
        requested_bytes: 0,
        freed_bytes: 0,
        success: 0,
        partial: 0,
        failed: 0,
        cancelled: 0,
        providers: Vec::new(),
        pre_allocated_sizing: false,
        entries: Vec::new(),
    }
}

fn push_entry(run: &mut CleanupRun, e: HistoryEntry) {
    if e.sizing == devclean_core::Sizing::Apparent {
        run.pre_allocated_sizing = true;
    }
    run.entry_count += 1;
    run.requested_bytes += e.requested_bytes;
    run.freed_bytes += e.freed_bytes;
    match e.status {
        CleanupStatus::Success => run.success += 1,
        CleanupStatus::Partial => run.partial += 1,
        CleanupStatus::Failed => run.failed += 1,
        CleanupStatus::Cancelled => run.cancelled += 1,
    }
    if !run.providers.contains(&e.provider) {
        run.providers.push(e.provider.clone());
    }
    run.entries.push(e);
}

fn read_all(path: &Path) -> Vec<HistoryEntry> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect()
}

const TAIL_CHUNK: usize = 64 * 1024;

/// Read whole lines from the end of the file until at least `min_runs`
/// distinct run keys have been seen (plus the run in progress, so a run
/// split across the chunk boundary is never truncated). Returns
/// newest-first. Falls back to a full read for small files.
fn read_tail_lines(path: &Path, min_runs: usize) -> Vec<HistoryEntry> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    if len <= TAIL_CHUNK as u64 {
        let mut all = read_all(path);
        all.reverse();
        return all;
    }

    let mut collected: Vec<HistoryEntry> = Vec::new();
    let mut seen_runs: Vec<String> = Vec::new();
    let mut chunk_start = len;
    let mut carry: Vec<u8> = Vec::new();

    while chunk_start > 0 {
        let read_len = TAIL_CHUNK.min(chunk_start as usize);
        chunk_start -= read_len as u64;
        if file.seek(SeekFrom::Start(chunk_start)).is_err() {
            return read_all(path).into_iter().rev().collect();
        }
        let mut buf = vec![0u8; read_len];
        if file.read_exact(&mut buf).is_err() {
            return read_all(path).into_iter().rev().collect();
        }
        // Carry the (possibly partial) first line of this chunk.
        buf.extend_from_slice(&carry);
        carry.clear();

        let text = String::from_utf8_lossy(&buf).into_owned();
        let mut lines: Vec<&str> = text.lines().collect();
        if chunk_start > 0 {
            // The first line here is cut in half — keep it for the next round.
            if let Some(first) = lines.first() {
                carry = first.as_bytes().to_vec();
                lines.remove(0);
            }
        }
        for line in lines.iter().rev() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(entry) = serde_json::from_str::<HistoryEntry>(line) else {
                continue; // corrupt line: skip, never fatal
            };
            let key = run_key(&entry);
            if !seen_runs.contains(&key) {
                // `min_runs` complete runs are enough — the (min_runs + 1)-th
                // key only proves the oldest one is finished.
                if seen_runs.len() > min_runs {
                    break;
                }
                seen_runs.push(key);
            }
            collected.push(entry);
        }
        if chunk_start == 0 {
            break;
        }
    }
    collected
}

#[cfg(test)]
mod tests {
    use super::*;
    use devclean_core::CleanupStatus;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("devclean-hist-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn entry(id: &str) -> HistoryEntry {
        HistoryEntry {
            run_id: "r1".to_string(),
            finished_at_unix: 1_700_000_000,
            resource_id: id.to_string(),
            provider: "node_modules".to_string(),
            name: format!("{id}/node_modules"),
            requested_bytes: 100,
            freed_bytes: 100,
            moved_to_trash: false,
            sizing: devclean_core::Sizing::Allocated,
            status: CleanupStatus::Success,
            error: None,
        }
    }

    /// Legacy line: written before `run_id` existed (contract < v0.6).
    fn legacy_entry(id: &str, finished_at_unix: u64) -> HistoryEntry {
        let mut e = entry(id);
        e.run_id = String::new();
        e.finished_at_unix = finished_at_unix;
        e
    }

    #[test]
    fn append_then_read_last_n_skipping_corrupt_lines() {
        use std::io::Write as _;
        let path = tmp("basic");
        for id in ["a", "b", "c"] {
            append_history_entry(&path, &entry(id)).unwrap();
        }
        // A corrupt line in the middle must be skipped, not fatal.
        {
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(f, "not json at all").unwrap();
        }
        append_history_entry(&path, &entry("d")).unwrap();
        let all = read_history(&path, 50);
        assert_eq!(all.len(), 4);
        assert_eq!(all[3].resource_id, "d");
        let last2 = read_history(&path, 2);
        assert_eq!(last2.len(), 2);
        assert_eq!(last2[0].resource_id, "c");
        assert_eq!(last2[1].resource_id, "d");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_reads_empty() {
        let missing = std::env::temp_dir().join("devclean-hist-definitely-missing.jsonl");
        let _ = std::fs::remove_file(&missing);
        assert!(read_history(&missing, 50).is_empty());
        assert!(read_history_runs(&missing, 10).is_empty());
    }

    /// The reported bug: one 289-resource cleanup consumed the whole
    /// 100-entry budget and hid every earlier run.
    #[test]
    fn a_huge_run_no_longer_hides_earlier_runs() {
        let path = tmp("huge");
        // Old small run: 1 entry, 10.8 GB.
        let mut big = entry("docker-0");
        big.run_id = "old".into();
        big.provider = "docker".into();
        big.requested_bytes = 10_800_000_000;
        big.freed_bytes = 10_800_000_000;
        append_history_entry(&path, &big).unwrap();
        // New huge run: 289 entries.
        for i in 0..289 {
            let mut e = entry(&format!("py-{i}"));
            e.run_id = "new".into();
            e.provider = "python_cache".into();
            append_history_entry(&path, &e).unwrap();
        }

        let runs = read_history_runs(&path, 20);
        assert_eq!(runs.len(), 2, "both runs must be visible");
        assert_eq!(runs[0].run_id, "new", "newest run first");
        assert_eq!(runs[0].entry_count, 289);
        assert_eq!(runs[0].freed_bytes, 28_900);
        assert_eq!(runs[1].run_id, "old");
        assert_eq!(runs[1].entry_count, 1);
        assert_eq!(
            runs[1].freed_bytes, 10_800_000_000,
            "the 10.8 GB run must report its real bytes"
        );
        // Global total across runs must match what actually happened.
        let total: u64 = runs.iter().map(|r| r.freed_bytes).sum();
        assert_eq!(total, 10_800_000_000 + 28_900);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn runs_respect_max_runs() {
        let path = tmp("max");
        for r in 0..10 {
            for i in 0..3 {
                let mut e = entry(&format!("r{r}-{i}"));
                e.run_id = format!("run{r}");
                e.finished_at_unix = 1_700_000_000 + r;
                append_history_entry(&path, &e).unwrap();
            }
        }
        let runs = read_history_runs(&path, 3);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].run_id, "run9");
        assert_eq!(runs[2].run_id, "run7");
        assert!(runs.iter().all(|r| r.entry_count == 3));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn legacy_lines_without_run_id_are_grouped_by_timestamp() {
        let path = tmp("legacy");
        // Two runs from before v0.6: 2 entries at t=100, 1 at t=200.
        append_history_entry(&path, &legacy_entry("a", 100)).unwrap();
        append_history_entry(&path, &legacy_entry("b", 100)).unwrap();
        append_history_entry(&path, &legacy_entry("c", 200)).unwrap();
        let runs = read_history_runs(&path, 10);
        assert_eq!(
            runs.len(),
            2,
            "legacy entries must not collapse into one run"
        );
        assert_eq!(runs[0].entry_count, 1);
        assert_eq!(runs[1].entry_count, 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn run_aggregates_counts_providers_and_errors() {
        let path = tmp("agg");
        for (i, status) in [
            CleanupStatus::Success,
            CleanupStatus::Partial,
            CleanupStatus::Failed,
            CleanupStatus::Cancelled,
        ]
        .into_iter()
        .enumerate()
        {
            let mut e = entry(&format!("x{i}"));
            e.provider = if i == 1 { "go_cache" } else { "docker" }.to_string();
            e.status = status;
            append_history_entry(&path, &e).unwrap();
        }
        let runs = read_history_runs(&path, 5);
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!((r.success, r.partial, r.failed, r.cancelled), (1, 1, 1, 1));
        assert_eq!(r.entry_count, 4);
        assert_eq!(r.providers.len(), 2);
        assert_eq!(r.providers[0], "docker");
        // Entries keep chronological order inside a run.
        assert_eq!(r.entries.first().unwrap().resource_id, "x0");
    }

    /// Tail-read must handle a file larger than one chunk, including lines
    /// that straddle a chunk boundary.
    #[test]
    fn tail_read_handles_multi_chunk_files_and_straddling_lines() {
        let path = tmp("tail");
        // ~10 runs × 400 entries with long names → comfortably > 64 KB.
        for r in 0..10 {
            for i in 0..400 {
                let mut e = entry(&format!("{}-{i}", "x".repeat(60)));
                e.run_id = format!("run{r}");
                e.finished_at_unix = 1_700_000_000 + r;
                append_history_entry(&path, &e).unwrap();
            }
        }
        let runs = read_history_runs(&path, 3);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].run_id, "run9");
        assert_eq!(runs[2].run_id, "run7");
        assert!(runs.iter().all(|r| r.entry_count == 400));
        assert_eq!(runs[0].requested_bytes, 400 * 100);
        let _ = std::fs::remove_file(&path);
    }
}
