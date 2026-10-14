//! Move-to-Trash for the default delete mode (v0.8.0).
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
//! ## Scope
//!
//! macOS only, matching the shipped platform (`tauri.conf.json` targets
//! `app`/`dmg`). Linux would need the freedesktop `.trashinfo` format to make
//! the file appear in a file manager's Trash; that is deliberately not
//! implemented rather than half-implemented, and is recorded as a non-goal.
//!
//! ## Recoverability
//!
//! The artifact lands in `~/.Trash` under its own name, so the user can drag
//! it back out. macOS "Put Back" metadata is not written, so Finder will not
//! offer the one-click undo — the file is still there and still recoverable.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The Trash directory for the current user.
pub fn default_trash_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join(".Trash"))
}

/// Move `path` into `trash_dir`, creating the directory if needed.
///
/// Falls back to a recursive copy when the source is on another filesystem:
/// `os.Rename` fails with `EXDEV` across volumes (an external drive, a
/// separate partition), and the Trash lives on the home volume. The original
/// is removed **only after** the copy fully succeeds, so a mid-copy failure
/// leaves the source intact.
pub fn move_to_trash(path: &Path, trash_dir: &Path) -> Result<PathBuf, String> {
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

    #[test]
    fn a_directory_lands_in_the_trash_and_leaves_its_original_path() {
        let (base, trash) = scratch("dir");
        let src = base.join("proj/node_modules");
        std::fs::create_dir_all(src.join("pkg")).unwrap();
        std::fs::write(src.join("pkg/index.js"), "x".repeat(500)).unwrap();

        let dest = move_to_trash(&src, &trash).expect("move");

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
    fn a_second_artifact_with_the_same_name_does_not_overwrite_the_first() {
        let (base, trash) = scratch("collide");
        for n in ["one", "two"] {
            let src = base.join(n).join("node_modules");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::write(src.join("which"), n).unwrap();
            move_to_trash(&src, &trash).expect("move");
        }
        let first = std::fs::read_to_string(trash.join("node_modules/which")).unwrap();
        let second = std::fs::read_to_string(trash.join("node_modules.1/which")).unwrap();
        assert_eq!(first, "one", "the first move must not be clobbered");
        assert_eq!(second, "two");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_trash_directory_is_created_on_demand() {
        let base = std::env::temp_dir().join(format!("devclean-trash-mk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("thing");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("f"), "x").unwrap();

        let trash = base.join("does/not/exist/yet");
        let dest = move_to_trash(&src, &trash).expect("move");
        assert!(dest.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_symlink_is_recreated_as_a_link_never_followed() {
        let (base, trash) = scratch("symlink");
        let src = base.join("tree");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(base.join("outside.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(base.join("outside.txt"), src.join("link")).unwrap();

        let dest = move_to_trash(&src, &trash).expect("move");
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
    fn a_missing_path_is_an_error_not_a_silent_success() {
        let (base, trash) = scratch("missing");
        let err = move_to_trash(&base.join("nope"), &trash).unwrap_err();
        assert!(err.contains("no longer exists"), "got: {err}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_single_file_can_be_trashed() {
        let (base, trash) = scratch("file");
        let f = base.join("settings.json");
        std::fs::write(&f, "{}").unwrap();
        let dest = move_to_trash(&f, &trash).expect("move");
        assert!(!f.exists());
        assert!(dest.is_file());
        let _ = std::fs::remove_dir_all(&base);
    }
}
