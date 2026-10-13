//! Cleanup history: append-only JSONL log (one [`HistoryEntry`] per line).
//!
//! Deliberately not SQLite: no new dependency, no migrations, and the log
//! is human-inspectable. Corrupt lines are skipped on read, never fatal.

use devclean_core::HistoryEntry;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
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
pub fn read_history(path: &Path, limit: usize) -> Vec<HistoryEntry> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let entries: Vec<HistoryEntry> = BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect();
    let skip = entries.len().saturating_sub(limit.max(1));
    entries.into_iter().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use devclean_core::CleanupStatus;

    fn entry(id: &str) -> HistoryEntry {
        HistoryEntry {
            finished_at_unix: 1_700_000_000,
            resource_id: id.to_string(),
            provider: "node_modules".to_string(),
            name: format!("{id}/node_modules"),
            requested_bytes: 100,
            freed_bytes: 100,
            status: CleanupStatus::Success,
            error: None,
        }
    }

    #[test]
    fn append_then_read_last_n_skipping_corrupt_lines() {
        use std::io::Write as _;
        let path = std::env::temp_dir().join(format!("devclean-hist-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
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
    }
}
