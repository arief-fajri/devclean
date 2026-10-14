//! Provider implementations. Every provider speaks through [`Provider`];
//! the backend never has `if node_modules / if docker` branches.

pub mod ai_guard;
pub mod cargo_registry;
pub mod claude;
pub mod docker;
pub mod docker_pruned;
pub mod global_cache;
pub mod go;
pub mod gradle_cache;
pub mod node;
pub mod npm_cache;
pub mod opencode_db;
pub mod opencode_fs;
pub mod provider;
pub mod python_cache;
pub mod registry;
pub mod rust_target;
pub mod xcode_data;

pub use ai_guard::{probe_process, sqlite_exclusive_available, ProcState};
pub use cargo_registry::CargoRegistryProvider;
pub use claude::ClaudeProvider;
pub use docker::DockerProvider;
pub use docker_pruned::DockerPrunedProvider;
pub use global_cache::GlobalCacheProvider;
pub use go::GoProvider;
pub use gradle_cache::GradleCacheProvider;
pub use node::NodeModulesProvider;
pub use npm_cache::NpmCacheProvider;
pub use opencode_db::OpencodeDbProvider;
pub use opencode_fs::OpencodeFsProvider;
pub use provider::{
    dir_size, dir_size_checked, home_dir, impact, measure_dir, project_detail, stale_note,
    stale_note_for_age, tool_output_dir, walk_cancelled, CleanupAction, Provider,
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
        Arc::new(DockerPrunedProvider),
        Arc::new(GlobalCacheProvider),
        Arc::new(NpmCacheProvider),
        Arc::new(PythonCacheProvider),
        Arc::new(CargoRegistryProvider),
        Arc::new(GradleCacheProvider),
        Arc::new(XcodeDataProvider),
        Arc::new(OpencodeFsProvider),
        Arc::new(ClaudeProvider),
        Arc::new(OpencodeDbProvider),
    ]
}
