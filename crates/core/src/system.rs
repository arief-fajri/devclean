//! Platform helpers. No assumption that cache locations are equal across
//! OSes — every path has a per-platform branch.

/// `"macos"`, `"windows"`, or `"linux"`.
pub fn platform_name() -> &'static str {
    std::env::consts::OS
}

/// Default roots DevClean scans: `~/Projects`, `~/Code`, `~/Workspace`,
/// filtered to directories that actually exist. Users can extend this in
/// Settings; the safety validator always re-checks containment.
pub fn default_scan_roots() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from));
    let Some(home) = home else {
        return Vec::new();
    };
    ["Projects", "Code", "Workspace"]
        .into_iter()
        .map(|d| home.join(d))
        .filter(|p| p.is_dir())
        .collect()
}
