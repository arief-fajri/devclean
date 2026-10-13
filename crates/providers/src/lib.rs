//! Provider implementations. Every provider speaks through [`Provider`];
//! the backend never has `if node_modules / if docker` branches.

pub mod cargo_registry;
pub mod docker;
pub mod go;
pub mod gradle_cache;
pub mod node;
pub mod npm_cache;
pub mod provider;
pub mod python_cache;
pub mod registry;
pub mod rust_target;
pub mod xcode_data;

pub use cargo_registry::CargoRegistryProvider;
pub use docker::DockerProvider;
pub use go::GoProvider;
pub use gradle_cache::GradleCacheProvider;
pub use node::NodeModulesProvider;
pub use npm_cache::NpmCacheProvider;
pub use provider::{
    dir_size, dir_size_checked, home_dir, project_detail, stale_note, stale_note_for_age,
    tool_output_dir, walk_cancelled, CleanupAction, Provider,
};
pub use python_cache::PythonCacheProvider;
pub use registry::ProviderRegistry;
pub use rust_target::RustTargetProvider;
pub use xcode_data::XcodeDataProvider;

/// All providers in scan order.
pub fn all_providers() -> Vec<std::sync::Arc<dyn Provider>> {
    use std::sync::Arc;
    vec![
        Arc::new(NodeModulesProvider),
        Arc::new(RustTargetProvider),
        Arc::new(GoProvider),
        Arc::new(DockerProvider),
        Arc::new(NpmCacheProvider),
        Arc::new(PythonCacheProvider),
        Arc::new(CargoRegistryProvider),
        Arc::new(GradleCacheProvider),
        Arc::new(XcodeDataProvider),
    ]
}
