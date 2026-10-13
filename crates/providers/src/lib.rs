//! Provider implementations. Every provider speaks through [`Provider`];
//! the backend never has `if node_modules / if docker` branches.

pub mod docker;
pub mod go;
pub mod node;
pub mod provider;
pub mod registry;
pub mod rust_target;

pub use docker::DockerProvider;
pub use go::GoProvider;
pub use node::NodeModulesProvider;
pub use provider::{dir_size, CleanupAction, Provider};
pub use registry::ProviderRegistry;
pub use rust_target::RustTargetProvider;

/// All MVP providers in scan order.
pub fn all_providers() -> Vec<std::sync::Arc<dyn Provider>> {
    use std::sync::Arc;
    vec![
        Arc::new(NodeModulesProvider),
        Arc::new(RustTargetProvider),
        Arc::new(GoProvider),
        Arc::new(DockerProvider),
    ]
}
