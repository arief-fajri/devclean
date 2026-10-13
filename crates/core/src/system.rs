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

/// Directory for DevClean's own files (settings.json, scan cache, cleanup
/// history). Per-platform, never inside a scan root. Created on demand.
pub fn app_data_dir() -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from));
    let base = match std::env::consts::OS {
        "macos" => home.map(|h| h.join("Library/Application Support")),
        "windows" => std::env::var_os("APPDATA").map(std::path::PathBuf::from),
        _ => std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| home.map(|h| h.join(".config"))),
    };
    base.unwrap_or_else(std::env::temp_dir).join("devclean")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_data_dir_is_namespaced() {
        assert_eq!(
            app_data_dir().file_name().and_then(|s| s.to_str()),
            Some("devclean")
        );
    }
}
