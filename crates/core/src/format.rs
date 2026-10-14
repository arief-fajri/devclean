//! Human-facing formatting shared by the Tauri shell and the menu bar.
//!
//! One implementation, because a number the user sees in the window and the
//! same number in the tray menu must never disagree (the tray reads the scan
//! cache directly, with no frontend in the loop).

/// `1.5 GB`, `840 MB`, `0 B`. Decimal units, matching the UI.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let i = ((bytes as f64).log(1000.0) as usize).min(UNITS.len() - 1);
    let v = bytes as f64 / 1000f64.powi(i as i32);
    format!("{v:.1} {}", UNITS[i])
}

/// Coarse "how long ago" for the menu bar: `just now`, `12m ago`, `3h ago`,
/// `5d ago`. Deliberately coarse — the menu bar is glanced at, not read, and
/// a false-precision age ("2h 47m") would imply a freshness we cannot promise.
pub fn format_age(secs: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    match secs {
        s if s < MINUTE => "just now".to_string(),
        s if s < HOUR => format!("{}m ago", s / MINUTE),
        s if s < DAY => format!("{}h ago", s / HOUR),
        s => format!("{}d ago", s / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_never_logged() {
        // log(0) is -inf; the zero case must not reach it.
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn formats_decimal_units() {
        assert_eq!(format_bytes(999), "999.0 B");
        assert_eq!(format_bytes(1_000), "1.0 KB");
        assert_eq!(format_bytes(1_500_000), "1.5 MB");
        assert_eq!(format_bytes(42_100_000_000), "42.1 GB");
    }

    #[test]
    fn caps_at_the_largest_unit() {
        assert!(format_bytes(u64::MAX).ends_with(" TB"));
    }

    #[test]
    fn age_is_coarse_and_never_says_something_it_does_not_know() {
        assert_eq!(format_age(0), "just now");
        assert_eq!(format_age(59), "just now");
        assert_eq!(format_age(60), "1m ago");
        assert_eq!(format_age(3 * 3600 + 59 * 60), "3h ago");
        assert_eq!(format_age(2 * 86_400), "2d ago");
    }
}
