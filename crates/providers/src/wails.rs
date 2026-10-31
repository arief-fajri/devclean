//! Wails build output. Two generations are supported because they use
//! different markers and different output paths:
//!
//! - **v2:** marker `wails.json`, output `build/bin/` and `frontend/dist/`.
//! - **v3:** no `wails.json`; the marker is a `go.mod` depending on
//!   `github.com/wailsapp/wails/v3` together with the v3 build config
//!   (`build/config.yml` or a `Taskfile.yml` that invokes `wails3`). Output
//!   lives in `bin/` at the project root, and the frontend bundle may be
//!   `frontend/dist/` (template default) or `dist/` at the root (custom
//!   frontend layout).
//!
//! The rest of `build/` — `config.yml`, `appicon.png`, the platform
//! templates and manifest stubs — is NOT output and is deliberately left
//! alone in both generations.
//!
//! This is an own-scanner rather than a tree rule: the marker sits at the
//! project root, while a [`crate::tree::TreeRule`] only inspects the matched
//! directory's parent. Matching on `build/` alone would offer committed
//! config in every project that happens to keep one.

use super::provider::{
    age_days, dir_size_detail_checked, impact, measure_dir, project_detail, CleanupAction,
    OwnScanner, Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct WailsProvider;

/// Directory names the marker walk never descends into: either huge, or
/// irrelevant to finding a project marker. A marker shipped inside a
/// dependency is not a project we own.
const PRUNE: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    "Pods",
    ".dart_tool",
    ".svelte-kit",
    ".next",
    ".turbo",
    ".expo",
    "build",
    "dist",
];

const V2_MARKER: &str = "wails.json";
const GO_MOD: &str = "go.mod";
const V3_MODULE: &str = "github.com/wailsapp/wails/v3";
const V3_CONFIG: &str = "build/config.yml";
const V3_TASKFILE: &str = "Taskfile.yml";
const V3_TASK_MARKER: &str = "wails3";

/// Deepest a marker may sit below a scan root (`root = 0`).
const MAX_DEPTH: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WailsKind {
    V2,
    V3,
}

/// Generated artifacts, relative to the project root. `build/bin` / `bin`
/// cost a full build to recreate (`caution`); frontend output is the
/// bundler's (`safe`).
struct Artifact {
    sub: &'static str,
    safety: SafetyLevel,
    consequence: &'static str,
    cost: &'static str,
    /// Root `dist/` is only the frontend bundle when the project keeps its
    /// `package.json` at the root; otherwise it may be unrelated output.
    root_package_json: bool,
}

const V2_ARTIFACTS: &[Artifact] = &[
    Artifact {
        sub: "build/bin",
        safety: SafetyLevel::Caution,
        consequence: "The compiled app only; your source and wails.json are untouched.",
        cost: "The next `wails build` recompiles Go and the frontend",
        root_package_json: false,
    },
    Artifact {
        sub: "frontend/dist",
        safety: SafetyLevel::Safe,
        consequence: "The frontend bundle only; your source is untouched.",
        cost: "The next build re-runs the frontend bundler",
        root_package_json: false,
    },
];

const V3_ARTIFACTS: &[Artifact] = &[
    Artifact {
        sub: "bin",
        safety: SafetyLevel::Caution,
        consequence: "The compiled app and packaged bundles only; your source, build config and Taskfile are untouched.",
        cost: "The next `wails3 build` recompiles Go and the frontend",
        root_package_json: false,
    },
    Artifact {
        sub: "frontend/dist",
        safety: SafetyLevel::Safe,
        consequence: "The frontend bundle only; your source is untouched.",
        cost: "The next build re-runs the frontend bundler",
        root_package_json: false,
    },
    Artifact {
        sub: "dist",
        safety: SafetyLevel::Safe,
        consequence: "The frontend bundle only; your source is untouched.",
        cost: "The next build re-runs the frontend bundler",
        root_package_json: true,
    },
];

fn artifacts(kind: WailsKind) -> &'static [Artifact] {
    match kind {
        WailsKind::V2 => V2_ARTIFACTS,
        WailsKind::V3 => V3_ARTIFACTS,
    }
}

/// True when `project` looks like a Wails v3 project. Both halves are
/// required: a Go module may depend on Wails without being an app, and a
/// `build/config.yml` may belong to an unrelated tool.
fn is_v3_project(project: &Path) -> bool {
    if !project.join(GO_MOD).is_file() {
        return false;
    }
    let has_build_config = project.join(V3_CONFIG).is_file();
    let has_taskfile = std::fs::read_to_string(project.join(V3_TASKFILE))
        .is_ok_and(|text| text.contains(V3_TASK_MARKER));
    if !has_build_config && !has_taskfile {
        return false;
    }
    std::fs::read_to_string(project.join(GO_MOD)).is_ok_and(|text| text.contains(V3_MODULE))
}

fn project_kind(project: &Path) -> Option<WailsKind> {
    if is_v3_project(project) {
        Some(WailsKind::V3)
    } else if project.join(V2_MARKER).is_file() {
        Some(WailsKind::V2)
    } else {
        None
    }
}

impl Provider for WailsProvider {
    fn id(&self) -> &'static str {
        "wails"
    }
    fn name(&self) -> &'static str {
        "Wails"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    /// Walk back from the artifact to the directory owning the marker
    /// (`build/bin`, `bin`, `dist` and `frontend/dist` all sit at most two
    /// levels below it).
    fn project_root(&self, artifact: &Path) -> Option<PathBuf> {
        artifact
            .ancestors()
            .skip(1)
            .take(3)
            .find(|p| project_kind(p).is_some())
            .map(Path::to_path_buf)
    }
}

impl WailsProvider {
    fn collect_project(
        &self,
        project: &Path,
        kind: WailsKind,
        cancel: &AtomicBool,
        out: &mut Vec<Resource>,
    ) {
        let project_name = project
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "?".to_string());
        let has_root_package_json = project.join("package.json").is_file();
        for artifact in artifacts(kind) {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            if artifact.root_package_json && !has_root_package_json {
                continue;
            }
            let dir = project.join(artifact.sub);
            if !dir.is_dir() {
                continue;
            }
            let Some(size) = dir_size_detail_checked(&dir, cancel) else {
                continue;
            };
            if size.allocated == 0 {
                continue;
            }
            let build_command = match kind {
                WailsKind::V2 => "wails build",
                WailsKind::V3 => "wails3 build",
            };
            out.push(Resource {
                id: String::new(),
                provider: self.id().to_string(),
                name: format!("{project_name}/{}", artifact.sub),
                path: dir,
                size_bytes: size.allocated,
                reclaimable_bytes: size.allocated,
                apparent_bytes: size.apparent_if_material(),
                inode_bytes: size.inode_bytes.clone(),
                safety: artifact.safety,
                detail: Some(project_detail(&project_name, project, size.allocated)),
                age_days: age_days(project),
                activity: None,
                last_commit_unix: None,
                blocked_reason: None,
                blocked_detail: None,
                impact: impact(
                    artifact.consequence,
                    Some(artifact.cost),
                    Some(build_command),
                ),
            });
        }
    }
}

/// Do not descend into the heavy or dependency directories above.
fn pruned(e: &walkdir::DirEntry) -> bool {
    // `depth() > 0` keeps a scan root that happens to be named "build" or
    // "dist" from being pruned before the walk starts.
    e.depth() > 0
        && e.file_type().is_dir()
        && e.file_name().to_str().is_some_and(|n| PRUNE.contains(&n))
}

impl OwnScanner for WailsProvider {
    fn scan(&self, roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        // BTreeMap keeps the order stable (reproducible opaque ids) and lets
        // a v3 marker win when a migrating project still has `wails.json`.
        let mut projects: BTreeMap<PathBuf, WailsKind> = BTreeMap::new();
        for root in roots {
            if cancel.load(Ordering::Relaxed) {
                return Vec::new();
            }
            let walker = walkdir::WalkDir::new(root)
                .follow_links(false)
                .max_depth(MAX_DEPTH)
                .into_iter()
                .filter_entry(|e| !pruned(e));
            for entry in walker {
                if cancel.load(Ordering::Relaxed) {
                    return Vec::new();
                }
                let Ok(e) = entry else { continue };
                if !e.file_type().is_file() {
                    continue;
                }
                let Some(name) = e.file_name().to_str() else {
                    continue;
                };
                let Some(project) = e.path().parent() else {
                    continue;
                };
                if name == V2_MARKER {
                    projects
                        .entry(project.to_path_buf())
                        .or_insert(WailsKind::V2);
                } else if name == GO_MOD && is_v3_project(project) {
                    projects.insert(project.to_path_buf(), WailsKind::V3);
                }
            }
        }

        let mut out = Vec::new();
        for (project, kind) in &projects {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            self.collect_project(project, *kind, cancel, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("devclean-wails-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    fn scan(root: &Path) -> Vec<Resource> {
        WailsProvider.scan(
            std::slice::from_ref(&root.to_path_buf()),
            &AtomicBool::new(false),
        )
    }

    fn v3_project(root: &Path, name: &str, with_build_config: bool) -> PathBuf {
        let proj = root.join(name);
        std::fs::create_dir_all(proj.join("build")).unwrap();
        std::fs::write(
            proj.join("go.mod"),
            "module example\nrequire github.com/wailsapp/wails/v3 v3.0.0\n",
        )
        .unwrap();
        if with_build_config {
            std::fs::write(proj.join("build/config.yml"), "version: '3'\n").unwrap();
        }
        proj
    }

    #[test]
    fn reports_build_bin_and_frontend_dist_but_not_the_rest_of_build() {
        let root = fixture("outputs");
        let proj = root.join("app");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("wails.json"), "{}").unwrap();
        write(&proj.join("build/bin/app.app/Contents/MacOS/app"), 1000);
        write(&proj.join("frontend/dist/assets/index.js"), 500);
        // Committed config in build/ must never be offered as output.
        write(&proj.join("build/appicon.png"), 900);
        write(&proj.join("build/darwin/Info.plist"), 700);
        write(&proj.join("src/main.go"), 300);

        let mut found = scan(&root);
        found.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(found.len(), 2, "{found:#?}");
        assert_eq!(found[0].name, "app/build/bin");
        assert_eq!(found[0].safety, SafetyLevel::Caution);
        assert_eq!(found[0].provider, "wails");
        assert_eq!(found[1].name, "app/frontend/dist");
        assert_eq!(found[1].safety, SafetyLevel::Safe);
        for r in &found {
            assert!(
                !r.path.ends_with("build") && !r.path.ends_with("appicon.png"),
                "the whole build/ tree must never be a resource: {}",
                r.path.display()
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_marker_means_nothing_is_reported() {
        let root = fixture("no-marker");
        write(&root.join("proj/build/bin/app"), 1000);
        write(&root.join("proj/frontend/dist/index.js"), 500);
        assert!(
            scan(&root).is_empty(),
            "a build/ without wails.json is not a Wails project"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn finds_a_nested_project_and_ignores_a_marker_inside_node_modules() {
        let root = fixture("nested");
        let inner = root.join("group").join("desk");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("wails.json"), "{}").unwrap();
        write(&inner.join("build/bin/app"), 400);
        // A dependency that happens to ship a wails.json must not be scanned.
        let dep = root.join("group/node_modules/dep");
        std::fs::create_dir_all(&dep).unwrap();
        std::fs::write(dep.join("wails.json"), "{}").unwrap();
        write(&dep.join("build/bin/app"), 400);

        let found = scan(&root);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].name, "desk/build/bin");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn project_root_walks_back_to_the_marker() {
        let root = fixture("root");
        let proj = root.join("app");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("wails.json"), "{}").unwrap();
        assert_eq!(
            WailsProvider.project_root(&proj.join("build/bin")),
            Some(proj.clone())
        );
        assert_eq!(
            WailsProvider.project_root(&proj.join("frontend/dist")),
            Some(proj)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_cancelled_scan_touches_nothing() {
        let root = fixture("cancel");
        let proj = root.join("app");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("wails.json"), "{}").unwrap();
        write(&proj.join("build/bin/app"), 1000);
        let cancel = AtomicBool::new(true);
        assert!(WailsProvider
            .scan(std::slice::from_ref(&root), &cancel)
            .is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Wails v3 has no `wails.json`. The project is identified by its Go
    /// module plus the v3 build configuration, and its output lives in
    /// `bin/` and (for root-frontend layouts) `dist/`.
    #[test]
    fn reports_v3_bin_and_root_dist_without_wails_json() {
        let root = fixture("v3-outputs");
        let proj = v3_project(&root, "app", true);
        write(&proj.join("package.json"), 50);
        write(&proj.join("bin/app.app/Contents/MacOS/app"), 1000);
        write(&proj.join("dist/index.html"), 500);
        // Committed config in build/ must never be offered as output.
        write(&proj.join("build/appicon.png"), 900);
        write(&proj.join("build/darwin/Info.plist"), 700);
        write(&proj.join("src/main.go"), 300);

        let mut found = scan(&root);
        found.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(found.len(), 2, "{found:#?}");
        assert_eq!(found[0].name, "app/bin");
        assert_eq!(found[0].safety, SafetyLevel::Caution);
        assert_eq!(found[1].name, "app/dist");
        assert_eq!(found[1].safety, SafetyLevel::Safe);
        for r in &found {
            assert!(
                !r.path.ends_with("build") && !r.path.ends_with("appicon.png"),
                "the whole build/ tree must never be a resource: {}",
                r.path.display()
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_taskfile_that_invokes_wails3_is_a_v3_marker() {
        let root = fixture("v3-taskfile");
        let proj = v3_project(&root, "app", false);
        std::fs::write(
            proj.join("Taskfile.yml"),
            "tasks:\n  build:\n    cmds:\n      - wails3 build\n",
        )
        .unwrap();
        write(&proj.join("bin/app"), 800);

        let found = scan(&root);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].name, "app/bin");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A Go module that merely depends on Wails is not necessarily an app.
    #[test]
    fn a_wails_dependency_without_the_v3_build_config_is_not_a_project() {
        let root = fixture("v3-no-config");
        let proj = v3_project(&root, "lib", false);
        write(&proj.join("package.json"), 50);
        write(&proj.join("bin/tool"), 800);
        write(&proj.join("dist/index.js"), 500);

        assert!(scan(&root).is_empty(), "not a Wails application");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn root_dist_requires_a_root_package_json() {
        let root = fixture("v3-no-package");
        let proj = v3_project(&root, "app", true);
        write(&proj.join("bin/app"), 800);
        write(&proj.join("dist/index.js"), 500);

        let found = scan(&root);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].name, "app/bin");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A project mid-migration may keep `wails.json`; the v3 marker wins so
    /// the wrong (`build/bin`) path is not offered.
    #[test]
    fn v3_wins_when_a_migrating_project_still_has_wails_json() {
        let root = fixture("v3-migrating");
        let proj = v3_project(&root, "app", true);
        std::fs::write(proj.join("wails.json"), "{}").unwrap();
        write(&proj.join("bin/app"), 800);
        write(&proj.join("build/bin/old"), 600);

        let found = scan(&root);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].name, "app/bin");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn project_root_walks_back_to_a_v3_marker() {
        let root = fixture("v3-root");
        let proj = v3_project(&root, "app", true);
        assert_eq!(
            WailsProvider.project_root(&proj.join("bin/app")),
            Some(proj.clone())
        );
        assert_eq!(WailsProvider.project_root(&proj.join("dist")), Some(proj));
        let _ = std::fs::remove_dir_all(&root);
    }
}
