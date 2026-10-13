//! Safety gate. Every destructive operation must pass through
//! [`SafetyValidator`] before touching the filesystem.
//!
//! Pipeline: `User request -> Provider -> SafetyValidator -> Execute`.
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

/// Validates cleanup targets against protected paths and scan roots.
#[derive(Debug, Clone)]
pub struct SafetyValidator {
    protected: Vec<PathBuf>,
    scan_roots: Vec<PathBuf>,
    /// Provider-owned global cache dirs (e.g. `~/.cargo/registry`), declared
    /// via `Provider::extra_allowed_roots`. Same protected-path rules apply;
    /// only the roots check is widened. Empty for scan-root-only providers.
    extra_roots: Vec<PathBuf>,
}

impl SafetyValidator {
    pub fn new(scan_roots: Vec<PathBuf>) -> Self {
        Self::new_with_extra_roots(scan_roots, Vec::new())
    }

    pub fn new_with_extra_roots(scan_roots: Vec<PathBuf>, extra_roots: Vec<PathBuf>) -> Self {
        let protected = default_protected_paths();
        let norm = |paths: Vec<PathBuf>| paths.into_iter().map(|p| normalize(&p)).collect();
        Self {
            protected,
            scan_roots: norm(scan_roots),
            extra_roots: norm(extra_roots),
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
    pub fn validate_path(&self, path: &Path) -> Result<(), SafetyError> {
        let path = normalize(path);

        for p in &self.protected {
            // Exact match: never clean a protected dir itself.
            if path == *p {
                return Err(SafetyError::Protected(path.display().to_string()));
            }
            // Inside a protected dir (e.g. `~/Documents/foo`). The filesystem
            // root `/` is exempt: every absolute path is "inside" it, so it
            // carries no signal (it is still covered by the exact match and
            // the ancestor check below).
            if p.parent().is_some() && path.starts_with(p) {
                return Err(SafetyError::Protected(path.display().to_string()));
            }
            // Ancestor of a protected dir (e.g. cleaning `$HOME` would wipe
            // `~/.ssh`): rejected, even if it sits inside a scan root (I7).
            if p.starts_with(&path) {
                return Err(SafetyError::Protected(path.display().to_string()));
            }
        }

        let inside_root = self
            .scan_roots
            .iter()
            .chain(self.extra_roots.iter())
            .any(|r| path.starts_with(r));
        if !inside_root {
            return Err(SafetyError::OutsideScanRoots(path.display().to_string()));
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
}
