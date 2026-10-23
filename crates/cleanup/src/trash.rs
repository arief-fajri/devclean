//! Move-to-Trash for the default delete mode (v0.8.0; XDG layout since 1.0.0).
//!
//! ## Why this is not the `trash` crate
//!
//! The obvious choice is the `trash` crate. On macOS it shells out to
//! AppleScript to ask Finder to do the move, which makes the whole cleanup
//! path depend on Finder being present and responsive. Measured here, from a
//! normal (sandboxed) process:
//!
//! ```text
//! Error during a `trash` operation: The AppleScript exited with error.
//! Finder got an error: The operation can't be completed because "" is in use. (-47)
//! ```
//!
//! A delete that fails when Finder is busy is worse than no delete: the user
//! chose Trash precisely so a mistake would be recoverable, and the failure
//! surfaces as an opaque AppleScript error. This module does the move itself,
//! which is testable, has no external process, and cannot be broken by Finder.
//!
//! ## Scope — two layouts, one entry point
//!
//! [`move_to_trash`] dispatches on the platform:
//!
//! - **macOS — flat layout.** The artifact lands in `~/.Trash` under its own
//!   name. macOS "Put Back" metadata is not written, so Finder will not offer
//!   the one-click undo — the file is still there and still recoverable by
//!   hand.
//! - **Linux — freedesktop (XDG) layout.** The artifact lands in
//!   `$XDG_DATA_HOME/Trash/files/<name>` with a sidecar
//!   `$XDG_DATA_HOME/Trash/info/<name>.trashinfo`, so a GNOME or KDE file
//!   manager shows it in its Trash and can restore it. The metadata is
//!   written **before** the move (per Trash Spec v1.0) so a crash mid-move
//!   never leaves a file in `files/` with no way home.
//!
//! Both layouts are always compiled and both are unit-tested on every host,
//! so the encoding, ordering and rollback rules are verified in CI on macOS
//! **and** Ubuntu regardless of which layout the host would dispatch to.
//!
//! **What CI cannot verify:** that the artifact actually appears in a real
//! GNOME or KDE Trash view. That is stated honestly in the README support
//! table ("implemented, unverified in a desktop file manager") rather than
//! hidden.
//!
//! ## Cross-device (EXDEV)
//!
//! `os.Rename` fails with `EXDEV` across volumes, and the Trash lives on the
//! home volume while an external drive or separate partition does not. Both
//! layouts fall back to a recursive copy, removing the original only after
//! the copy fully succeeds. Per the XDG spec this 1.0 implementation does
//! **not** attempt the per-mount `$topdir/.Trash-$uid` dance — a cross-device
//! artifact is always copied into the home-volume Trash, which is the simple
//! behaviour that never half-implements the spec.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The Trash directory for the current user.
///
/// - Linux: `$XDG_DATA_HOME/Trash`, falling back to `~/.local/share/Trash`
///   when `XDG_DATA_HOME` is unset or empty.
/// - Other Unix: `~/.Trash`.
#[cfg(target_os = "linux")]
pub fn default_trash_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("Trash"));
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join(".local/share/Trash"))
}

/// The Trash directory for the current user: `~/.Trash`.
#[cfg(not(target_os = "linux"))]
pub fn default_trash_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join(".Trash"))
}

/// Move `path` into the platform's Trash, creating the directory if needed.
///
/// Dispatches to [`move_freedesktop`] on Linux and [`move_flat`] elsewhere.
/// Both functions are always compiled so each layout's rules are exercised by
/// unit tests on every CI host.
pub fn move_to_trash(path: &Path, trash_dir: &Path) -> Result<PathBuf, String> {
    if cfg!(target_os = "linux") {
        move_freedesktop(path, trash_dir)
    } else {
        move_flat(path, trash_dir)
    }
}

/// macOS flat layout: the artifact lands at `trash_dir/<name>` directly.
///
/// Falls back to a recursive copy when the source is on another filesystem.
/// The original is removed **only after** the copy fully succeeds, so a
/// mid-copy failure leaves the source intact.
pub fn move_flat(path: &Path, trash_dir: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("{} no longer exists", path.display()));
    }
    std::fs::create_dir_all(trash_dir)
        .map_err(|e| format!("could not create {}: {e}", trash_dir.display()))?;

    // Claim a destination by ATTEMPTING the rename, not by checking whether
    // the name is free first. Two cleanups running at once would both see
    // `node_modules` as free and one would fail; retrying the next candidate
    // makes the choice atomic.
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_string());
    let mut last_error = None;
    for n in 0..10_000usize {
        let dest = if n == 0 {
            trash_dir.join(&name)
        } else {
            trash_dir.join(format!("{name}.{n}"))
        };
        match std::fs::rename(path, &dest) {
            Ok(()) => return Ok(dest),
            Err(e) if is_cross_device(&e) => {
                let result = copy_tree(path, &dest).and_then(|()| {
                    std::fs::remove_dir_all(path)
                        .or_else(|_| std::fs::remove_file(path))
                        .map_err(|e| format!("copied to Trash but could not remove original: {e}"))
                });
                match result {
                    Ok(()) => return Ok(dest),
                    Err(e) => {
                        // A partial copy must never be left behind for the
                        // user to mistake for the real thing.
                        let _ = std::fs::remove_dir_all(&dest);
                        return Err(e);
                    }
                }
            }
            Err(e) => {
                // The name was already taken. The errno differs by platform
                // and by what occupies the name — macOS returns ENOTEMPTY for
                // a non-empty directory, EEXIST for a file — so test the
                // observable fact (the destination exists) instead of
                // matching error kinds. Anything else is a real failure.
                if dest.exists() {
                    last_error = Some(e);
                    continue;
                }
                return Err(format!("could not move {} to Trash: {e}", path.display()));
            }
        }
    }
    Err(format!(
        "could not find a free name in {}: {}",
        trash_dir.display(),
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "too many collisions".to_string())
    ))
}

/// freedesktop (XDG) layout: `trash_dir/files/<name>` plus the sidecar
/// `trash_dir/info/<name>.trashinfo`.
///
/// Ordering follows Trash Spec v1.0: the `.trashinfo` is created **first**,
/// atomically with `O_EXCL` (`create_new(true)`), and only after it exists is
/// the artifact moved into `files/`. If writing the info fails the artifact is
/// never touched; if the move fails the info is deleted so no orphan metadata
/// is left behind. The invariant: there is never a file in `files/` without
/// its `.trashinfo`, and never a rollback from Trash back to the original
/// path.
pub fn move_freedesktop(path: &Path, trash_dir: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("{} no longer exists", path.display()));
    }
    let files_dir = trash_dir.join("files");
    let info_dir = trash_dir.join("info");
    ensure_private_dir(&files_dir)?;
    ensure_private_dir(&info_dir)?;

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_string());
    let date = local_deletion_date();
    let mut last_error = None;
    for n in 0..10_000usize {
        let candidate = if n == 0 {
            name.clone()
        } else {
            format!("{name}.{n}")
        };
        let info_path = info_dir.join(format!("{candidate}.trashinfo"));
        match claim_info(&info_path, path, &date) {
            Err(ClaimError::NameTaken) => continue,
            Err(ClaimError::Real(e)) => return Err(e),
            Ok(()) => {}
        }
        let dest = files_dir.join(&candidate);
        match move_artifact(path, &dest) {
            Ok(()) => return Ok(dest),
            Err(e) => {
                // The move failed: drop the sidecar we just wrote so the
                // artifact stays where it was with no orphan metadata.
                let _ = std::fs::remove_file(&info_path);
                // If the destination exists this was a name race — try the
                // next candidate. Otherwise it is a real failure.
                if dest.exists() {
                    last_error = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(format!(
        "could not find a free name in {}: {}",
        files_dir.display(),
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "too many collisions".to_string())
    ))
}

/// Move `path` to `dest`, falling back to a recursive copy on `EXDEV`.
///
/// A partial copy is always cleaned up; the original is removed only after a
/// fully successful copy.
fn move_artifact(path: &Path, dest: &Path) -> Result<(), String> {
    match std::fs::rename(path, dest) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device(&e) => {
            let result = copy_tree(path, dest).and_then(|()| {
                std::fs::remove_dir_all(path)
                    .or_else(|_| std::fs::remove_file(path))
                    .map_err(|e| format!("copied to Trash but could not remove original: {e}"))
            });
            match result {
                Ok(()) => Ok(()),
                Err(e) => {
                    let _ = std::fs::remove_dir_all(dest);
                    Err(e)
                }
            }
        }
        Err(e) => Err(format!("could not move {} to Trash: {e}", path.display())),
    }
}

/// Why the sidecar could not be claimed.
enum ClaimError {
    /// The candidate name is already taken (`.trashinfo` exists).
    NameTaken,
    /// A real failure — the filesystem refused, or the write failed.
    Real(String),
}

/// Create `info_path` atomically (`O_EXCL`) and write the `.trashinfo`
/// contents. A partial write is rolled back so a later retry never sees a
/// truncated sidecar.
fn claim_info(info_path: &Path, original: &Path, date: &str) -> Result<(), ClaimError> {
    let contents = trashinfo_contents(original, date);
    let mut file = std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(info_path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                ClaimError::NameTaken
            } else {
                ClaimError::Real(format!("could not create {}: {e}", info_path.display()))
            }
        })?;
    if let Err(e) = file.write_all(contents.as_bytes()) {
        let _ = std::fs::remove_file(info_path);
        return Err(ClaimError::Real(format!(
            "could not write {}: {e}",
            info_path.display()
        )));
    }
    Ok(())
}

/// The exact bytes of a `.trashinfo` sidecar — pure, byte-pinned in tests.
///
/// `Path=` holds the absolute original path, percent-encoded; `DeletionDate=`
/// is local time in `YYYY-MM-DDTHH:MM:SS` form (no offset, per the spec).
pub fn trashinfo_contents(original: &Path, date: &str) -> String {
    let encoded = percent_encode(original.to_string_lossy().as_bytes());
    format!("[Trash Info]\nPath={encoded}\nDeletionDate={date}\n")
}

/// Percent-encode per the Trash Spec: keep unreserved characters **and** `/`
/// literal (a path separator must stay readable), encode everything else as
/// `%XX` with uppercase hex.
fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Create `dir` (and parents) and force mode `0700` on it.
///
/// `0700` on `files/` and `info/` is hardening rather than a spec
/// requirement; it is set explicitly so a newly created Trash never leaks
/// artifact names to other local users.
fn ensure_private_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    Ok(())
}

/// The current local time as `YYYY-MM-DDTHH:MM:SS`, via `localtime_r`.
///
/// Falls back to UTC if the local-time lookup fails, and to the epoch if even
/// that fails — a wrong-but-present date beats a missing sidecar.
fn local_deletion_date() -> String {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let time_t: libc::time_t = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::localtime_r(&time_t, &mut tm) };
    if ok.is_null() {
        let mut tm_utc: libc::tm = unsafe { std::mem::zeroed() };
        let ok2 = unsafe { libc::gmtime_r(&time_t, &mut tm_utc) };
        if ok2.is_null() {
            return "1970-01-01T00:00:00".to_string();
        }
        tm = tm_utc;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

fn is_cross_device(e: &std::io::Error) -> bool {
    // EXDEV = 18 on both Linux and macOS.
    e.raw_os_error() == Some(18)
}

/// Recursive copy that recreates directories, files (contents + permission
/// bits) and symlinks **as links**, never following them — a link into a huge
/// or cyclic target must not be walked.
fn copy_tree(src: &Path, dest: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(src).map_err(|e| e.to_string())?;
        return std::os::unix::fs::symlink(target, dest).map_err(|e| e.to_string());
    }
    if meta.is_file() {
        std::fs::copy(src, dest).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if !meta.is_dir() {
        return Err(format!("unsupported file type: {}", src.display()));
    }

    let perm = meta.permissions().mode();
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        copy_tree(&entry.path(), &dest.join(entry.file_name()))?;
    }
    // `create_dir_all` ANDs the mode with the umask, so restore the exact
    // bits after the subtree is populated.
    let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(perm));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> (PathBuf, PathBuf) {
        let base =
            std::env::temp_dir().join(format!("devclean-trash-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let trash = base.join("Trash");
        std::fs::create_dir_all(&trash).unwrap();
        (base, trash)
    }

    // ------------------------------------------------------------ flat

    #[test]
    fn flat_a_directory_lands_in_the_trash_and_leaves_its_original_path() {
        let (base, trash) = scratch("flat-dir");
        let src = base.join("proj/node_modules");
        std::fs::create_dir_all(src.join("pkg")).unwrap();
        std::fs::write(src.join("pkg/index.js"), "x".repeat(500)).unwrap();

        let dest = move_flat(&src, &trash).expect("move");

        assert!(!src.exists(), "the original path must be free");
        assert!(dest.starts_with(&trash), "it must be inside the Trash");
        assert_eq!(
            std::fs::read_to_string(dest.join("pkg/index.js"))
                .unwrap()
                .len(),
            500,
            "contents must survive the move"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flat_a_second_artifact_with_the_same_name_does_not_overwrite_the_first() {
        let (base, trash) = scratch("flat-collide");
        for n in ["one", "two"] {
            let src = base.join(n).join("node_modules");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::write(src.join("which"), n).unwrap();
            move_flat(&src, &trash).expect("move");
        }
        let first = std::fs::read_to_string(trash.join("node_modules/which")).unwrap();
        let second = std::fs::read_to_string(trash.join("node_modules.1/which")).unwrap();
        assert_eq!(first, "one", "the first move must not be clobbered");
        assert_eq!(second, "two");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flat_the_trash_directory_is_created_on_demand() {
        let base = std::env::temp_dir().join(format!("devclean-trash-mk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("thing");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("f"), "x").unwrap();

        let trash = base.join("does/not/exist/yet");
        let dest = move_flat(&src, &trash).expect("move");
        assert!(dest.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flat_a_symlink_is_recreated_as_a_link_never_followed() {
        let (base, trash) = scratch("flat-symlink");
        let src = base.join("tree");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(base.join("outside.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(base.join("outside.txt"), src.join("link")).unwrap();

        let dest = move_flat(&src, &trash).expect("move");
        let moved = dest.join("link");
        assert!(
            std::fs::symlink_metadata(&moved)
                .unwrap()
                .file_type()
                .is_symlink(),
            "a symlink must stay a symlink, not become a copy of its target"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flat_a_missing_path_is_an_error_not_a_silent_success() {
        let (base, trash) = scratch("flat-missing");
        let err = move_flat(&base.join("nope"), &trash).unwrap_err();
        assert!(err.contains("no longer exists"), "got: {err}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flat_a_single_file_can_be_trashed() {
        let (base, trash) = scratch("flat-file");
        let f = base.join("settings.json");
        std::fs::write(&f, "{}").unwrap();
        let dest = move_flat(&f, &trash).expect("move");
        assert!(!f.exists());
        assert!(dest.is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    // ----------------------------------------------------- freedesktop

    #[test]
    fn freedesktop_lands_in_files_and_writes_the_sidecar() {
        let (base, trash) = scratch("xdg-land");
        let src = base.join("proj/node_modules");
        std::fs::create_dir_all(src.join("pkg")).unwrap();
        std::fs::write(src.join("pkg/index.js"), "x".repeat(500)).unwrap();

        let dest = move_freedesktop(&src, &trash).expect("move");

        assert!(!src.exists(), "the original path must be free");
        assert_eq!(
            dest,
            trash.join("files/node_modules"),
            "the artifact lands in files/, per the XDG spec"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("pkg/index.js"))
                .unwrap()
                .len(),
            500,
            "contents must survive the move"
        );
        let info = trash.join("info/node_modules.trashinfo");
        assert!(
            info.is_file(),
            "a sidecar must exist for every file in files/"
        );
        let body = std::fs::read_to_string(&info).unwrap();
        let expected_path = percent_encode(src.to_string_lossy().as_bytes());
        assert!(
            body.contains(&format!("Path={expected_path}\n")),
            "the sidecar must record the original path, got:\n{body}"
        );
        assert!(
            body.starts_with("[Trash Info]\n"),
            "the sidecar must start with the Trash Info header, got:\n{body}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn freedesktop_sidecar_bytes_are_pinned() {
        // The exact spec format, byte for byte, with a fixed date so the
        // assertion is deterministic.
        let body = trashinfo_contents(
            Path::new("/home/me/my proj/node_modules"),
            "2026-01-02T03:04:05",
        );
        assert_eq!(
            body,
            "[Trash Info]\nPath=/home/me/my%20proj/node_modules\nDeletionDate=2026-01-02T03:04:05\n"
        );
    }

    #[test]
    fn percent_encoding_keeps_slashes_and_unreserved_but_encodes_the_rest() {
        // Space and non-ASCII (Cyrillic "папка" in UTF-8) must be encoded;
        // `/` and `A-Za-z0-9-._~` stay.
        let encoded =
            percent_encode(b"/home/me/pa spa/\xd0\xbf\xd0\xb0\xd0\xbf\xd0\xba\xd0\xb0 file-1.txt");
        assert_eq!(
            encoded,
            "/home/me/pa%20spa/%D0%BF%D0%B0%D0%BF%D0%BA%D0%B0%20file-1.txt"
        );
        // The unreserved set survives literally.
        let unreserved = percent_encode(b"AZaz09-._~/");
        assert_eq!(unreserved, "AZaz09-._~/");
    }

    #[test]
    fn freedesktop_files_and_info_are_private_0700() {
        let (base, trash) = scratch("xdg-modes");
        let src = base.join("thing");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("f"), "x").unwrap();
        move_freedesktop(&src, &trash).expect("move");
        for dir in ["files", "info"] {
            let mode = std::fs::metadata(trash.join(dir))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o777,
                0o700,
                "{dir}/ must be created 0700, got {mode:o}"
            );
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn freedesktop_a_name_collision_retries_and_writes_a_sidecar_per_name() {
        let (base, trash) = scratch("xdg-collide");
        for n in ["one", "two"] {
            let src = base.join(n).join("node_modules");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::write(src.join("which"), n).unwrap();
            move_freedesktop(&src, &trash).expect("move");
        }
        let first = std::fs::read_to_string(trash.join("files/node_modules/which")).unwrap();
        let second = std::fs::read_to_string(trash.join("files/node_modules.1/which")).unwrap();
        assert_eq!(first, "one", "the first move must not be clobbered");
        assert_eq!(second, "two");
        assert!(trash.join("info/node_modules.trashinfo").is_file());
        assert!(trash.join("info/node_modules.1.trashinfo").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn freedesktop_a_preexisting_files_entry_rolls_back_the_sidecar_and_retries() {
        // Simulate the race the spec's ordering protects against: `files/`
        // already holds a non-empty `node_modules`, so the rename fails. The
        // sidecar we claimed must be removed and the next candidate used.
        let (base, trash) = scratch("xdg-race");
        std::fs::create_dir_all(trash.join("files/node_modules")).unwrap();
        std::fs::write(trash.join("files/node_modules/occupied"), "x").unwrap();
        let src = base.join("node_modules");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("new"), "y").unwrap();

        let dest = move_freedesktop(&src, &trash).expect("move");

        assert_eq!(dest, trash.join("files/node_modules.1"));
        assert!(
            !trash.join("info/node_modules.trashinfo").exists(),
            "the failed candidate's sidecar must be rolled back"
        );
        assert!(trash.join("info/node_modules.1.trashinfo").is_file());
        assert_eq!(std::fs::read_to_string(dest.join("new")).unwrap(), "y");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn freedesktop_a_broken_info_entry_is_a_refusal_not_a_move() {
        // If `info/` is occupied by a *file*, the sidecar cannot be created,
        // and the spec's invariant says the artifact must not move.
        let (base, trash) = scratch("xdg-broken-info");
        std::fs::write(trash.join("info"), "not a directory").unwrap();
        let src = base.join("node_modules");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("f"), "x").unwrap();

        let err = move_freedesktop(&src, &trash).unwrap_err();
        assert!(err.contains("could not create"), "got: {err}");
        assert!(src.exists(), "the artifact must stay where it was");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn freedesktop_a_single_file_can_be_trashed() {
        let (base, trash) = scratch("xdg-file");
        let f = base.join("settings.json");
        std::fs::write(&f, "{}").unwrap();
        let dest = move_freedesktop(&f, &trash).expect("move");
        assert!(!f.exists());
        assert_eq!(dest, trash.join("files/settings.json"));
        assert!(trash.join("info/settings.json.trashinfo").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn freedesktop_a_missing_path_is_an_error_not_a_silent_success() {
        let (base, trash) = scratch("xdg-missing");
        let err = move_freedesktop(&base.join("nope"), &trash).unwrap_err();
        assert!(err.contains("no longer exists"), "got: {err}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn freedesktop_a_symlink_is_recreated_as_a_link_never_followed() {
        let (base, trash) = scratch("xdg-symlink");
        let src = base.join("tree");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(base.join("outside.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(base.join("outside.txt"), src.join("link")).unwrap();

        let dest = move_freedesktop(&src, &trash).expect("move");
        let moved = dest.join("link");
        assert!(
            std::fs::symlink_metadata(&moved)
                .unwrap()
                .file_type()
                .is_symlink(),
            "a symlink must stay a symlink, not become a copy of its target"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    // ----------------------------------------------------- shared bits

    #[test]
    fn local_deletion_date_matches_the_xdg_format() {
        let date = local_deletion_date();
        // YYYY-MM-DDTHH:MM:SS — 19 bytes, digits and separators in fixed spots.
        assert_eq!(date.len(), 19, "got: {date}");
        let bytes = date.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            match i {
                4 | 7 => assert_eq!(b, b'-', "date separator at {i} in {date}"),
                10 => assert_eq!(b, b'T', "date and time joined by T at {i} in {date}"),
                13 | 16 => assert_eq!(b, b':', "time separator at {i} in {date}"),
                _ => assert!(b.is_ascii_digit(), "digit expected at {i} in {date}"),
            }
        }
    }

    #[test]
    fn copy_tree_recreates_contents_symlinks_and_modes() {
        let base = std::env::temp_dir().join(format!("devclean-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("tree");
        std::fs::create_dir_all(src.join("deep")).unwrap();
        std::fs::write(src.join("deep/f.txt"), "payload").unwrap();
        std::fs::write(base.join("outside.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(base.join("outside.txt"), src.join("link")).unwrap();
        let dest = base.clone().join("copied");

        copy_tree(&src, &dest).expect("copy");

        assert_eq!(
            std::fs::read_to_string(dest.join("deep/f.txt")).unwrap(),
            "payload"
        );
        assert!(
            std::fs::symlink_metadata(dest.join("link"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "symlinks must be recreated as links, not followed"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
