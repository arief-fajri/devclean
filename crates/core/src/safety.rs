//! Safety gate. Every destructive operation must pass through
//! [`SafetyValidator`] before touching the filesystem.
//!
//! Pipeline: `User request -> Provider -> SafetyValidator -> Execute`.
//!
//! Every check runs on the lexical spelling AND on the canonical
//! (symlink-resolved) form, so a symlink into protected territory cannot
//! open a back door.
//!
//! Invariants (see `docs/development/methodology/system-model.md`):
//! - I1: the frontend never supplies a raw path; it supplies a resource id.
//! - I2: deletion only happens for validator-approved paths.
//! - I7: protected paths are never deletable, even if a provider claims them.

use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SafetyError {
    #[error("path is protected and must never be cleaned: {0}")]
    Protected(String),
    #[error("path is outside the configured scan roots: {0}")]
    OutsideScanRoots(String),
    #[error("unknown resource id: {0}")]
    UnknownResource(String),
}

/// One reference path stored in both forms the validator compares against:
/// the lexically normalized spelling, and the symlink-resolved (canonical)
/// physical location. They differ exactly when the path walks through a
/// symlink — the gap a purely lexical check cannot see.
#[derive(Debug, Clone)]
struct PathEntry {
    lexical: PathBuf,
    canonical: PathBuf,
}

impl PathEntry {
    fn new(path: &Path) -> Self {
        let lexical = normalize(path);
        let canonical = canonicalize_best_effort(&lexical);
        Self { lexical, canonical }
    }
}

/// Validates cleanup targets against protected paths and scan roots.
#[derive(Debug, Clone)]
pub struct SafetyValidator {
    protected: Vec<PathEntry>,
    scan_roots: Vec<PathEntry>,
    /// Provider-owned global cache dirs (e.g. `~/.cargo/registry`), declared
    /// via `Provider::extra_allowed_roots`. Same protected-path rules apply;
    /// only the roots check is widened. Empty for scan-root-only providers.
    extra_roots: Vec<PathEntry>,
}

impl SafetyValidator {
    pub fn new(scan_roots: Vec<PathBuf>) -> Self {
        Self::new_with_extra_roots(scan_roots, Vec::new())
    }

    pub fn new_with_extra_roots(scan_roots: Vec<PathBuf>, extra_roots: Vec<PathBuf>) -> Self {
        Self::with_protected(default_protected_paths(), scan_roots, extra_roots)
    }

    /// Construct a validator with an explicit protected list. Production code
    /// seeds it from [`default_protected_paths`]; this exists for tests and
    /// future per-platform extension.
    pub fn with_protected(
        protected: Vec<PathBuf>,
        scan_roots: Vec<PathBuf>,
        extra_roots: Vec<PathBuf>,
    ) -> Self {
        let entries = |paths: Vec<PathBuf>| paths.iter().map(|p| PathEntry::new(p)).collect();
        Self {
            protected: entries(protected),
            scan_roots: entries(scan_roots),
            extra_roots: entries(extra_roots),
        }
    }

    /// Resolve + validate a resource id via a caller-supplied registry lookup.
    /// The registry (owned by `src-tauri`) maps opaque ids to real paths, so
    /// path resolution stays server-side (guardrail G-SAFE-01).
    pub fn resolve(
        &self,
        resource_id: &str,
        lookup: impl FnOnce(&str) -> Option<PathBuf>,
    ) -> Result<PathBuf, SafetyError> {
        let path = lookup(resource_id)
            .ok_or_else(|| SafetyError::UnknownResource(resource_id.to_string()))?;
        self.validate_path(&path)?;
        Ok(path)
    }

    /// Core check: path must be inside a scan root and must not touch a
    /// protected path in either direction (equal, inside, or ancestor-of).
    ///
    /// The check runs twice: once on the lexical spelling the provider
    /// reported, and once on the canonical (symlink-resolved) form, so a
    /// scan root or path component that is a symlink into protected
    /// territory (e.g. `~/Dev → ~/Documents/Dev`) is refused too.
    pub fn validate_path(&self, path: &Path) -> Result<(), SafetyError> {
        let lexical = normalize(path);
        self.check_form(&lexical, false)?;
        let canonical = canonicalize_best_effort(&lexical);
        if canonical != lexical {
            self.check_form(&canonical, true)?;
        }
        Ok(())
    }

    /// One pass of the three-direction protected check plus the scan-root
    /// allow-list, against one form of every reference path.
    fn check_form(&self, path: &Path, canonical: bool) -> Result<(), SafetyError> {
        fn field(e: &PathEntry, canonical: bool) -> &Path {
            if canonical {
                e.canonical.as_path()
            } else {
                e.lexical.as_path()
            }
        }
        let shown = path.display().to_string();

        for entry in &self.protected {
            let p = field(entry, canonical);
            // Exact match: never clean a protected dir itself.
            if path == p {
                return Err(SafetyError::Protected(shown));
            }
            // Inside a protected dir (e.g. `~/Documents/foo`). The filesystem
            // root `/` is exempt: every absolute path is "inside" it, so it
            // carries no signal (it is still covered by the exact match and
            // the ancestor check below).
            if p.parent().is_some() && path.starts_with(p) {
                return Err(SafetyError::Protected(shown));
            }
            // Ancestor of a protected dir (e.g. cleaning `$HOME` would wipe
            // `~/.ssh`): rejected, even if it sits inside a scan root (I7).
            if p.starts_with(path) {
                return Err(SafetyError::Protected(shown));
            }
        }

        let inside_root = self
            .scan_roots
            .iter()
            .chain(self.extra_roots.iter())
            .any(|r| path.starts_with(field(r, canonical)));
        if !inside_root {
            return Err(SafetyError::OutsideScanRoots(shown));
        }

        Ok(())
    }
}

/// Minimal protected set (see `docs/security.md`).
/// Callers extend this per-platform in the future (`platform/` modules).
pub fn default_protected_paths() -> Vec<PathBuf> {
    let home = home_dir();
    let mut out = vec![PathBuf::from("/")];
    if let Some(h) = home {
        for sub in [
            "Documents",
            "Desktop",
            "Downloads",
            "Pictures",
            "Videos",
            ".ssh",
            ".gnupg",
            ".config",
        ] {
            out.push(h.join(sub));
        }
    }
    out
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Lexical normalization (no IO): resolves `.`, `..`, and duplicate
/// separators so prefix comparison is sound without hitting the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        out
    }
}

/// Resolve a path through symlinks as far as it physically exists. When the
/// leaf (or part of the chain) is missing — already cleaned, or a name the
/// provider reported that was never created — the nearest existing ancestor
/// is resolved and the remaining components are appended, so every symlink
/// that DOES exist is still seen. Nothing to resolve falls back to the
/// lexical form.
fn canonicalize_best_effort(path: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(path) {
        return c;
    }
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor: &Path = path;
    while let Some(name) = cursor.file_name() {
        tail.push(name.to_os_string());
        let Some(parent) = cursor.parent() else {
            break;
        };
        cursor = parent;
        if let Ok(c) = std::fs::canonicalize(cursor) {
            let mut out = c;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return normalize(&out);
        }
    }
    normalize(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validator() -> SafetyValidator {
        SafetyValidator::new(vec![PathBuf::from("/Users/test/Projects")])
    }

    #[test]
    fn rejects_filesystem_root() {
        let v = validator();
        assert_eq!(
            v.validate_path(Path::new("/")),
            Err(SafetyError::Protected("/".to_string()))
        );
    }

    #[test]
    fn rejects_home_as_ancestor_of_protected() {
        // Even if HOME were a scan root, deleting it would wipe protected
        // children, so ancestor-of-protected is rejected (I7).
        let home = home_dir().expect("test needs HOME set");
        let v = SafetyValidator::new(vec![home.clone()]);
        assert!(matches!(
            v.validate_path(&home),
            Err(SafetyError::Protected(_))
        ));
    }

    #[test]
    fn rejects_ssh_and_documents_children() {
        let home = home_dir().expect("test needs HOME set");
        let v = SafetyValidator::new(vec![home.clone()]);
        assert!(v.validate_path(&home.join(".ssh/id_rsa")).is_err());
        assert!(v.validate_path(&home.join("Documents/notes")).is_err());
    }

    #[test]
    fn rejects_path_outside_scan_roots() {
        let v = validator();
        assert_eq!(
            v.validate_path(Path::new("/tmp/random-cache")),
            Err(SafetyError::OutsideScanRoots(
                "/tmp/random-cache".to_string()
            ))
        );
    }

    #[test]
    fn accepts_project_cache_inside_scan_root() {
        let v = validator();
        assert!(v
            .validate_path(Path::new("/Users/test/Projects/my-app/node_modules"))
            .is_ok());
    }

    #[test]
    fn resolve_rejects_unknown_id() {
        let v = validator();
        assert_eq!(
            v.resolve("nope", |_| None),
            Err(SafetyError::UnknownResource("nope".to_string()))
        );
    }

    #[test]
    fn resolve_validates_registry_path() {
        let v = validator();
        // Registry claims a path outside scan roots -> rejected, not executed.
        let r = v.resolve("evil", |_| Some(PathBuf::from("/etc")));
        assert!(r.is_err());
    }

    #[test]
    fn extra_roots_allow_only_the_declared_cache_dir() {
        let home = home_dir().expect("test needs HOME set");
        let registry = home.join(".cargo/registry");
        let v = SafetyValidator::new_with_extra_roots(
            vec![PathBuf::from("/Users/test/Projects")],
            vec![registry.clone()],
        );
        // The declared global cache dir is cleanable…
        assert!(v.validate_path(&registry).is_ok());
        assert!(v.validate_path(&registry.join("cache/x")).is_ok());
        // …but siblings under ~/.cargo are not (e.g. installed binaries).
        assert!(v.validate_path(&home.join(".cargo/bin/rustc")).is_err());
        // …and protected paths stay untouchable from any direction.
        assert!(v.validate_path(&home.join(".ssh")).is_err());
    }

    /// Symlink regression tests (feedback verifikasi v1.0.0): a purely
    /// lexical validator lets a scan root or path component that is a
    /// symlink into protected territory pass. Unix-only (symlink API).
    #[cfg(unix)]
    mod symlink_tests {
        use super::*;
        use std::os::unix::fs::symlink;

        fn scratch(tag: &str) -> PathBuf {
            let root =
                std::env::temp_dir().join(format!("devclean-safety-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            root
        }

        fn expect_protected(v: &SafetyValidator, path: &Path) {
            match v.validate_path(path) {
                Err(SafetyError::Protected(_)) => {}
                other => panic!("expected Protected for {}, got {other:?}", path.display()),
            }
        }

        #[test]
        fn scenario_a_scan_root_that_is_a_symlink_into_a_protected_dir_is_refused() {
            let tmp = scratch("scan-root-link");
            let protected = tmp.join("Documents");
            let dev = protected.join("Dev");
            let victim = dev.join("node_modules");
            std::fs::create_dir_all(&victim).unwrap();
            std::fs::write(dev.join("IMPORTANT.txt"), "keep").unwrap();
            let link = tmp.join("scanlink");
            symlink(&dev, &link).unwrap();

            let v =
                SafetyValidator::with_protected(vec![protected], vec![link.clone()], Vec::new());
            expect_protected(&v, &link.join("node_modules"));
            // Validation never touches the disk.
            assert!(victim.exists());
            assert!(dev.join("IMPORTANT.txt").exists());
            let _ = std::fs::remove_dir_all(&tmp);
        }

        #[test]
        fn scenario_a_is_still_refused_when_the_leaf_does_not_exist_yet() {
            let tmp = scratch("scan-root-link-missing");
            let protected = tmp.join("Documents");
            let dev = protected.join("Dev");
            std::fs::create_dir_all(&dev).unwrap();
            let link = tmp.join("scanlink");
            symlink(&dev, &link).unwrap();

            let v = SafetyValidator::with_protected(vec![protected], vec![link], Vec::new());
            // node_modules was never created; the parent walk still sees the
            // symlink and refuses.
            expect_protected(&v, &tmp.join("scanlink/node_modules"));
            let _ = std::fs::remove_dir_all(&tmp);
        }

        #[test]
        fn scenario_b_symlink_mid_path_inside_a_scan_root_is_refused() {
            let tmp = scratch("mid-link");
            let protected = tmp.join("Documents");
            std::fs::create_dir_all(protected.join("Dev")).unwrap();
            let scan = tmp.join("Projects");
            std::fs::create_dir_all(&scan).unwrap();
            symlink(protected.join("Dev"), scan.join("link")).unwrap();

            let v =
                SafetyValidator::with_protected(vec![protected], vec![scan.clone()], Vec::new());
            expect_protected(&v, &scan.join("link/node_modules"));
            let _ = std::fs::remove_dir_all(&tmp);
        }

        #[test]
        fn scenario_c_target_that_is_itself_a_symlink_is_resolved_before_the_check() {
            let tmp = scratch("target-link");
            let protected = tmp.join("Documents");
            std::fs::create_dir_all(protected.join("Dev")).unwrap();
            let scan = tmp.join("Projects");
            std::fs::create_dir_all(&scan).unwrap();
            symlink(protected.join("Dev"), scan.join("evil_link")).unwrap();
            let real = scan.join("real");
            std::fs::create_dir_all(&real).unwrap();
            symlink(&real, scan.join("good_link")).unwrap();

            let v =
                SafetyValidator::with_protected(vec![protected], vec![scan.clone()], Vec::new());
            // The link lives inside the scan root but RESOLVES into protected
            // territory: refused (over-strict on purpose — remove_dir_all
            // would only unlink, but validation never has to guess).
            expect_protected(&v, &scan.join("evil_link"));
            // A link that resolves inside the scan root stays cleanable.
            assert!(v.validate_path(&scan.join("good_link")).is_ok());
            let _ = std::fs::remove_dir_all(&tmp);
        }

        #[test]
        fn missing_targets_are_still_validated_lexically() {
            let v = SafetyValidator::with_protected(
                vec![PathBuf::from("/Users/test/Documents")],
                vec![PathBuf::from("/Users/test/Projects")],
                Vec::new(),
            );
            // Nonexistent path inside the scan root: cleanable.
            assert!(v
                .validate_path(Path::new("/Users/test/Projects/my-app/node_modules"))
                .is_ok());
            // Nonexistent path spelled inside a protected dir: refused.
            assert!(v
                .validate_path(Path::new("/Users/test/Documents/notes"))
                .is_err());
        }
    }
}
