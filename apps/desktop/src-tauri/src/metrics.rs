//! Memory measurement, so "how much did that cost?" has an answer instead of
//! an estimate.
//!
//! Two numbers, both from `getrusage`:
//!
//! * [`MemSample::self_peak`] — the high-water mark of **this** process.
//! * [`MemSample::child_peak`] — the high-water mark of the heaviest child we
//!   have reaped, which during a cleanup is `docker builder prune` / `go
//!   clean` / `npm`. That is where the memory actually goes during cleaning;
//!   our own footprint barely moves.
//!
//! ## What these numbers are not
//!
//! `ru_maxrss` is a **high-water mark for the whole process lifetime**, not a
//! current reading and not an average. Two consequences worth stating plainly:
//!
//! * It never decreases. The useful figure is therefore the **delta** between
//!   two samples — that is how much the peak grew while a phase was running.
//! * It says nothing about the WebKit process that renders the UI. WKWebView
//!   is launched by the system, not by us, so `RUSAGE_CHILDREN` does not see
//!   it either. Measure that one externally with `tools/measure_footprint.sh`.

/// A memory reading, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemSample {
    /// Peak RSS of this process so far.
    pub self_peak: u64,
    /// Peak RSS of the heaviest child process reaped so far.
    pub child_peak: u64,
}

impl MemSample {
    pub fn capture() -> Self {
        Self {
            self_peak: peak(Who::Self_).unwrap_or(0),
            child_peak: peak(Who::Children).unwrap_or(0),
        }
    }

    /// How much our own peak grew since `earlier`. This is the number to read,
    /// not the absolute peak: an absolute high-water mark says where the
    /// process has ever been, not what a phase cost.
    pub fn self_growth_since(&self, earlier: &MemSample) -> u64 {
        self.self_peak.saturating_sub(earlier.self_peak)
    }

    /// Same, for the heaviest child (docker/go/npm during a cleanup).
    pub fn child_growth_since(&self, earlier: &MemSample) -> u64 {
        self.child_peak.saturating_sub(earlier.child_peak)
    }

    /// One log line: what the phase cost, in MB.
    pub fn delta_line(&self, earlier: &MemSample, phase: &str) -> String {
        // `{:.1}` so every line reads the same width; plain Display would
        // print "2 MB" next to "2.5 MB".
        format!(
            "{phase}: self peak +{:.1} MB, heaviest child +{:.1} MB (abs self {:.1} MB, child {:.1} MB)",
            mb(self.self_growth_since(earlier)),
            mb(self.child_growth_since(earlier)),
            mb(self.self_peak),
            mb(self.child_peak)
        )
    }
}

/// Bytes → MB, one decimal, for log lines.
pub fn mb(bytes: u64) -> f64 {
    (bytes as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0
}

/// Which resource-usage counter to read.
#[derive(Clone, Copy)]
enum Who {
    Self_,
    Children,
}

#[cfg(all(unix, not(target_os = "macos")))]
fn peak(target: Who) -> Option<u64> {
    // Linux reports ru_maxrss in kilobytes; macOS in bytes.
    let which = match target {
        Who::Self_ => libc::RUSAGE_SELF,
        Who::Children => libc::RUSAGE_CHILDREN,
    };
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(which, &mut usage) } != 0 {
        return None;
    }
    Some(usage.ru_maxrss as u64 * 1024)
}

#[cfg(target_os = "macos")]
fn peak(target: Who) -> Option<u64> {
    // macOS reports ru_maxrss in bytes.
    let which = match target {
        Who::Self_ => libc::RUSAGE_SELF,
        Who::Children => libc::RUSAGE_CHILDREN,
    };
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(which, &mut usage) } != 0 {
        return None;
    }
    Some(usage.ru_maxrss as u64)
}

#[cfg(not(unix))]
fn peak(_target: Who) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn megabytes_round_to_one_decimal() {
        assert_eq!(mb(0), 0.0);
        assert_eq!(mb(1024 * 1024), 1.0);
        assert_eq!(mb(1536 * 1024), 1.5);
        assert_eq!(mb(1024 * 1024 * 1024), 1024.0);
    }

    #[test]
    fn growth_is_a_difference_and_never_negative() {
        let before = MemSample {
            self_peak: 100,
            child_peak: 50,
        };
        let after = MemSample {
            self_peak: 250,
            child_peak: 50,
        };
        assert_eq!(after.self_growth_since(&before), 150);
        // The high-water mark never falls; reporting a negative would be a lie.
        assert_eq!(after.child_growth_since(&before), 0);
    }

    #[test]
    fn the_delta_line_names_the_phase_and_both_numbers() {
        let before = MemSample {
            self_peak: 0,
            child_peak: 0,
        };
        let after = MemSample {
            self_peak: 2 * 1024 * 1024,
            child_peak: 1024 * 1024,
        };
        let line = after.delta_line(&before, "cleanup");
        assert!(line.contains("cleanup"), "{line}");
        assert!(line.contains("self peak +2.0 MB"), "{line}");
        assert!(line.contains("heaviest child +1.0 MB"), "{line}");
    }

    #[test]
    fn a_real_reading_is_positive() {
        // Guards against a silently broken rusage call returning zeros, which
        // would make every future "0.0 MB" reading look plausible.
        let s = MemSample::capture();
        if cfg!(unix) {
            assert!(s.self_peak > 0, "getrusage returned nothing: {s:?}");
        }
    }
}
