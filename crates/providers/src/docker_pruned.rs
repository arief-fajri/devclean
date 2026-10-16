//! Docker images and volumes — the data `cleanup.sh` reclaims via
//! `docker system prune -a --volumes` and DevClean previously ignored
//! entirely (16.7 GB on this machine).
//!
//! ## Safety posture
//!
//! These are the most destructive things DevClean can do, so they are
//! gated three ways, all in the engine rather than the UI:
//!
//! 1. `Settings.docker_prune_scope` must include them (default: `BuildCache`,
//!    which does NOT).
//! 2. `allow_privileged_operations` must be on (default: off).
//! 3. The resource is `SafetyLevel::Dangerous`, so the confirmation dialog
//!    treats it as such.
//!
//! Each level is a SEPARATE resource with its own size, so the user can
//! choose to free images without touching a volume that might hold a
//! database.

use super::provider::{impact, CleanupAction, Provider};
use devclean_core::{DockerPruneScope, Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Sentinel paths — Docker resources have no directory we own.
const IMAGES_PATH: &str = "__docker__/images";
const VOLUMES_PATH: &str = "__docker__/volumes";

pub struct DockerPrunedProvider;

fn docker_available() -> bool {
    std::process::Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

impl Provider for DockerPrunedProvider {
    fn id(&self) -> &'static str {
        "docker_pruned"
    }
    fn name(&self) -> &'static str {
        "Docker images & volumes"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Dangerous
    }

    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) || !docker_available() {
            return Vec::new();
        }
        let Some(text) = super::docker::df_text() else {
            return Vec::new();
        };
        [
            (
                IMAGES_PATH,
                "Images",
                "Images",
                SafetyLevel::Caution,
                "Unused images are removed. Tagged images you built by hand go too, \
                 and the next build starts from the Dockerfile again.",
                "Requires network to re-pull base images",
                DockerPruneScope::BuildCacheImages,
                "docker image prune -af",
            ),
            (
                VOLUMES_PATH,
                "Local volumes",
                "Local Volumes",
                SafetyLevel::Dangerous,
                "Unused volumes are REMOVED. A volume can hold database state \
                 that no image references any more — there is no way to \
                 recover it once deleted.",
                "Any data in an unreferenced volume is lost",
                DockerPruneScope::All,
                "docker volume prune -af",
            ),
        ]
        .into_iter()
        .filter_map(
            |(sentinel, name, row, safety, consequence, cost, scope, cmd)| {
                let bytes = crate::docker::reclaimable_for(&text, row)?;
                if bytes == 0 {
                    return None;
                }
                Some(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: format!("Docker {name}"),
                    path: PathBuf::from(sentinel),
                    size_bytes: bytes,
                    reclaimable_bytes: bytes,
                    // External figure (`docker system df`), not a tree walk:
                    // there is no separate logical size to report.
                    apparent_bytes: None,
                    safety,
                    detail: Some(format!(
                        "Needs Docker scope “{}” and Privileged operations",
                        scope_label(scope)
                    )),
                    age_days: None,
                    activity: None,
                    last_commit_unix: None,
                    blocked_reason: None,
                    blocked_detail: None,
                    impact: impact(consequence, Some(cost), Some(cmd)),
                })
            },
        )
        .collect()
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        if path.to_string_lossy().contains("volumes") {
            CleanupAction::RunCommand {
                program: "docker",
                args: &["volume", "prune", "-f"],
            }
        } else {
            CleanupAction::RunCommand {
                program: "docker",
                args: &["image", "prune", "-af"],
            }
        }
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        if !docker_available() {
            return None;
        }
        let text = super::docker::df_text()?;
        let row = if path.to_string_lossy().contains("volumes") {
            "Local Volumes"
        } else {
            "Images"
        };
        crate::docker::reclaimable_for(&text, row)
    }

    /// Declares the widest scope this provider can act on. The engine then
    /// requires both that scope to be selected AND the privileged opt-in.
    fn required_docker_scope(&self) -> Option<DockerPruneScope> {
        Some(DockerPruneScope::All)
    }
}

fn scope_label(s: DockerPruneScope) -> &'static str {
    match s {
        DockerPruneScope::Off => "off",
        DockerPruneScope::BuildCache => "build cache",
        DockerPruneScope::BuildCacheImages => "build cache + images",
        DockerPruneScope::All => "build cache + images + volumes",
    }
}

/// Keeps the sentinel constant referenced in tests.
#[allow(dead_code)]
const _SENTINELS: [&str; 2] = [IMAGES_PATH, VOLUMES_PATH];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker;

    const REAL: &str = concat!(
        r#"{"Active":"3","Reclaimable":"11.95GB (83%)","Size":"14.38GB","TotalCount":"17","Type":"Images"}"#,
        "\n",
        r#"{"Active":"3","Reclaimable":"4.768GB (83%)","Size":"5.698GB","TotalCount":"17","Type":"Local Volumes"}"#,
        "\n",
        r#"{"Active":"0","Reclaimable":"0B","Size":"0B","TotalCount":"0","Type":"Build Cache"}"#,
    );

    #[test]
    fn image_and_volume_reclaimable_are_read_from_df() {
        assert_eq!(
            crate::docker::reclaimable_for(REAL, "Images"),
            Some(11_950_000_000)
        );
        assert_eq!(
            crate::docker::reclaimable_for(REAL, "Local Volumes"),
            Some(4_768_000_000)
        );
        assert_eq!(crate::docker::reclaimable_for(REAL, "Build Cache"), Some(0));
        assert_eq!(docker::reclaimable_for(REAL, "Nope"), None);
    }

    /// The reason this provider exists: those 16.7 GB used to be invisible.
    #[test]
    fn the_wider_scopes_surface_what_build_cache_alone_hides() {
        let build_cache = docker::reclaimable_for(REAL, "Build Cache").unwrap();
        let images = docker::reclaimable_for(REAL, "Images").unwrap();
        let volumes = docker::reclaimable_for(REAL, "Local Volumes").unwrap();
        assert_eq!(build_cache, 0);
        assert!(images > 10_000_000_000);
        assert!(volumes > 4_000_000_000);
    }

    #[test]
    fn each_sentinel_maps_to_its_own_fixed_command() {
        let p = DockerPrunedProvider;
        assert!(matches!(
            p.cleanup_action(Path::new(IMAGES_PATH)),
            CleanupAction::RunCommand {
                args: &["image", "prune", "-af"],
                ..
            }
        ));
        assert!(matches!(
            p.cleanup_action(Path::new(VOLUMES_PATH)),
            CleanupAction::RunCommand {
                args: &["volume", "prune", "-f"],
                ..
            }
        ));
    }

    #[test]
    fn this_provider_demands_the_widest_scope() {
        assert_eq!(
            DockerPrunedProvider.required_docker_scope(),
            Some(DockerPruneScope::All)
        );
    }

    /// G-SAFE-03: the scan is harmless, but cleanup needs BOTH the scope
    /// and the privileged flag. Asserted through the engine elsewhere; this
    /// pins that the provider is the thing asking for `All`.
    #[test]
    fn dangerous_is_the_default_safety() {
        assert_eq!(
            DockerPrunedProvider.default_safety(),
            SafetyLevel::Dangerous
        );
    }
}
