//! Free space on a volume, measured without spawning anything.
//!
//! This used to shell out to `df -k`. That was a subprocess on the boot path
//! (the window awaits it while loading settings) with no deadline — the same
//! shape of problem as G-REL-05 — and it could only ever describe `$HOME`.
//! `statvfs(2)` answers the same question in-process, in microseconds.
//!
//! ## What the numbers mean
//!
//! `total` is every block on the filesystem; `available` is `f_bavail`, the
//! blocks an ordinary user may actually write — not `f_bfree`, which includes
//! the blocks reserved for root. That is why this matches the "Available"
//! column `df` prints rather than its raw free count.

use devclean_core::DiskFree;
#[cfg(not(unix))]
use std::path::Path;

#[cfg(unix)]
mod imp {
    use super::DiskFree;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    fn c_path(path: &Path) -> Option<CString> {
        CString::new(path.as_os_str().as_bytes()).ok()
    }

    /// `(available, total)` in bytes, or `None` if the path cannot be probed.
    pub fn space(path: &Path) -> Option<(u64, u64)> {
        let c = c_path(path)?;
        let mut buf: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut buf) } != 0 {
            return None;
        }
        // f_frsize is the fragment size POSIX says to scale by; fall back to
        // the fundamental block size when a platform reports zero.
        let block = if buf.f_frsize != 0 {
            buf.f_frsize as u64
        } else {
            buf.f_bsize as u64
        };
        if block == 0 {
            return None;
        }
        Some((
            (buf.f_bavail as u64).saturating_mul(block),
            (buf.f_blocks as u64).saturating_mul(block),
        ))
    }

    pub fn mount(path: &Path) -> Option<String> {
        mount_impl(path)
    }

    /// macOS: match the device holding `path` against the mount table to learn
    /// where that device is mounted. `/dev/disk3s5` on its own tells a user
    /// nothing; `/Volumes/Work` does.
    ///
    /// The field names read backwards on Darwin: `f_mntonname` is the
    /// directory mounted **on**, `f_mntfromname` is the device mounted
    /// **from**. Verified against a live table — a live volume reports
    /// `f_mntfromname = "/dev/disk3s5"` and `f_mntonname =
    /// "/System/Volumes/Data"`.
    /// `getmntinfo` hands back a pointer into a lazily allocated static
    /// buffer, and the scan, the command and a cleanup can all reach this at
    /// once. Hold the lock across the walk and copy out an owned String.
    #[cfg(target_os = "macos")]
    fn mount_impl(path: &Path) -> Option<String> {
        use std::ffi::CStr;
        use std::sync::Mutex;

        static TABLE: Mutex<()> = Mutex::new(());

        let c = c_path(path)?;
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        let device = unsafe { CStr::from_ptr(st.f_mntfromname.as_ptr()) }.to_string_lossy();

        let _guard = TABLE.lock().ok()?;
        let mut table: *mut libc::statfs = std::ptr::null_mut();
        let count = unsafe { libc::getmntinfo(&mut table, libc::MNT_NOWAIT) };
        if count <= 0 || table.is_null() {
            return None;
        }
        for i in 0..count as usize {
            let entry = unsafe { &*table.add(i) };
            let name = unsafe { CStr::from_ptr(entry.f_mntfromname.as_ptr()) }.to_string_lossy();
            if name == device {
                let dir = unsafe { CStr::from_ptr(entry.f_mntonname.as_ptr()) }.to_string_lossy();
                let dir = dir.into_owned();
                return (dir != "/").then_some(dir);
            }
        }
        None
    }

    /// Elsewhere the mount table is not reachable through libc in a form worth
    /// carrying, so the card falls back to describing the probe instead of
    /// guessing a name.
    #[cfg(not(target_os = "macos"))]
    fn mount_impl(_path: &Path) -> Option<String> {
        None
    }

    pub fn describe(path: &Path) -> Option<DiskFree> {
        let (available_bytes, total_bytes) = space(path)?;
        Some(DiskFree {
            available_bytes,
            total_bytes,
            known: true,
            mount: mount(path),
        })
    }
}

/// Not unix: no `statvfs`, so there is nothing to read. Windows keeps the old
/// `df` probe (below) rather than silently losing the card it already had.
#[cfg(not(unix))]
mod imp {
    use super::DiskFree;
    use std::path::Path;

    pub fn describe(_path: &Path) -> Option<DiskFree> {
        None
    }

    pub fn space(_path: &Path) -> Option<(u64, u64)> {
        None
    }

    pub fn mount(_path: &Path) -> Option<String> {
        None
    }
}

/// The volume that holds the user's home directory.
///
/// This is the volume the card describes. It is *not* necessarily where the
/// scan roots live — a root on an external drive is a different volume, which
/// is why the mount name travels with the numbers.
pub fn home_volume() -> Option<DiskFree> {
    let home = devclean_core::home_dir()?;
    imp::describe(&home)
}

/// The `df` fallback for platforms without `statvfs` (Windows in practice).
///
/// Kept byte-for-byte compatible with the previous behaviour, including
/// `known: false` when the probe fails, so nothing regresses on a platform that
/// never had a working `df`.
#[cfg(not(unix))]
pub fn df_fallback(path: &Path) -> Option<DiskFree> {
    let out = std::process::Command::new("df")
        .args(["-k", &path.display().to_string()])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let last = text.lines().skip(1).find(|l| !l.starts_with("map "))?;
    let cols: Vec<&str> = last.split_whitespace().collect();
    Some(DiskFree {
        available_bytes: cols.get(3)?.parse::<u64>().ok()?.saturating_mul(1024),
        total_bytes: cols.get(1)?.parse::<u64>().ok()?.saturating_mul(1024),
        known: true,
        mount: None,
    })
}

/// The volume the app should report, whatever the platform.
pub fn report() -> DiskFree {
    if let Some(free) = home_volume() {
        return free;
    }
    #[cfg(not(unix))]
    if let Some(free) = df_fallback(&std::path::PathBuf::from("/")) {
        return free;
    }
    DiskFree {
        available_bytes: 0,
        total_bytes: 0,
        known: false,
        mount: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The device a path lives on, as the mount table names it.
    #[cfg(target_os = "macos")]
    fn device_of(path: &Path) -> String {
        use std::ffi::{CStr, CString};
        use std::os::unix::ffi::OsStrExt;
        let c = CString::new(path.as_os_str().as_bytes()).unwrap();
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::statfs(c.as_ptr(), &mut st) }, 0, "{path:?}");
        unsafe { CStr::from_ptr(st.f_mntfromname.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn the_home_volume_has_plausible_space() {
        let Some(free) = home_volume() else {
            // A machine without a resolvable $HOME has nothing to check.
            return;
        };
        assert!(free.known);
        assert!(
            free.total_bytes > 1024 * 1024 * 1024,
            "implausible total: {}",
            free.total_bytes
        );
        assert!(
            free.available_bytes <= free.total_bytes,
            "available {} > total {}",
            free.available_bytes,
            free.total_bytes
        );
        // Not the classic "a parse failure silently became 0" bug.
        assert!(free.available_bytes > 0);
    }

    #[test]
    fn a_path_that_does_not_exist_is_unknown_not_zero() {
        let missing = Path::new("/devclean-definitely-not-here-9f2a");
        assert!(imp::space(missing).is_none());
        assert!(imp::describe(missing).is_none());
    }

    /// The old doc comment on `DiskFree` said "the volume holding the app's
    /// data". That is true on macOS — but only because DevClean puts its data
    /// under `$HOME`. This is the test that keeps the claim honest.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_app_data_directory_really_is_on_the_home_volume() {
        let home = devclean_core::home_dir().expect("home");
        let data = devclean_core::app_data_dir();
        assert!(
            data.starts_with(&home),
            "app data {} escaped home {} — the two may now be different volumes",
            data.display(),
            home.display()
        );
        assert_eq!(imp::mount(&home), imp::mount(&data));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_can_name_the_mount_of_the_home_volume() {
        let home = devclean_core::home_dir().unwrap();
        let mount = imp::mount(&home).expect("a mount name");
        // A mount *point*, not a device: the two fields of Darwin's `statfs`
        // are easy to swap, and `/dev/disk3s5` is not something to show a user.
        assert!(mount.starts_with('/'), "not a path: {mount}");
        assert!(
            !mount.starts_with("/dev/"),
            "device name, not a mount: {mount}"
        );
        assert_ne!(mount, "/", "a bare root names nothing");
        // Not a prefix check: macOS firmlinks make `/Users/...` a synthetic
        // path over `/System/Volumes/Data`, so `$HOME` never *starts* with the
        // mount point. The device is what actually has to match.
        assert_eq!(device_of(&home), device_of(Path::new(&mount)));
    }

    /// The whole reason for this module: `statvfs` must agree with what `df`
    /// told the user before. Compared on the home volume, where both probe the
    /// same filesystem.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn statvfs_agrees_with_df() {
        let home = devclean_core::home_dir().expect("home");
        let (available, total) = imp::space(&home).expect("space");

        let Ok(out) = std::process::Command::new("df")
            .args(["-k", &home.display().to_string()])
            .output()
        else {
            return; // no `df` on this machine: nothing to compare against
        };
        if !out.status.success() {
            return;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let Some(last) = text.lines().skip(1).find(|l| !l.starts_with("map ")) else {
            return;
        };
        let cols: Vec<&str> = last.split_whitespace().collect();
        let df_avail: u64 = cols[3].parse().expect("df available");
        let df_total: u64 = cols[1].parse().expect("df total");

        let df_avail_bytes = df_avail.saturating_mul(1024);
        let df_total_bytes = df_total.saturating_mul(1024);

        // Free space moves while the test runs, so compare loosely on the
        // available figure and tightly on the total (which cannot change
        // under a scan).
        let drift = (df_avail_bytes as f64 - available as f64).abs() / df_avail_bytes as f64;
        assert!(
            drift < 0.02,
            "available {} vs df {} — {:.2}% apart",
            available,
            df_avail_bytes,
            drift * 100.0
        );
        let total_drift = (df_total_bytes as f64 - total as f64).abs() / df_total_bytes as f64;
        assert!(
            total_drift < 0.001,
            "total {} vs df {} — block size disagrees",
            total,
            df_total_bytes
        );
    }
}
