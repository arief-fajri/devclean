//! Provider implementations. Every provider speaks through [`Provider`];
//! the backend never has `if node_modules / if docker` branches.

pub mod ai_guard;
pub mod android;
pub mod browser_temp;
pub mod cargo_registry;
pub mod claude;
pub mod docker;
pub mod docker_pruned;
pub mod exec;
pub mod flutter;
pub mod git;
pub mod global_cache;
pub mod go;
pub mod go_vendor;
pub mod gradle_cache;
pub mod llm_models;
pub mod node;
pub mod npm_cache;
pub mod opencode_db;
pub mod opencode_fs;
pub mod pods;
pub mod provider;
pub mod python_cache;
pub mod registry;
pub mod ruby;
pub mod rust_target;
pub mod tree;
pub mod xcode_data;

pub use ai_guard::{probe_process, sqlite_exclusive_available, ProcState};
pub use android::AndroidProvider;
pub use browser_temp::{browsers_running, floor_safety, safety_for, BrowserTempProvider};
pub use cargo_registry::CargoRegistryProvider;
pub use claude::ClaudeProvider;
pub use docker::DockerProvider;
pub use docker_pruned::DockerPrunedProvider;
pub use exec::{run_with_deadline, run_with_input};
pub use flutter::FlutterProvider;
pub use git::{git_available, ignored_paths, is_dirty, last_commit_unix, repo_root_for};
pub use global_cache::GlobalCacheProvider;
pub use go::GoProvider;
pub use go_vendor::GoVendorProvider;
pub use gradle_cache::GradleCacheProvider;
pub use llm_models::LlmModelsProvider;
pub use node::NodeModulesProvider;
pub use npm_cache::NpmCacheProvider;
pub use opencode_db::OpencodeDbProvider;
pub use opencode_fs::OpencodeFsProvider;
pub use pods::PodsProvider;
pub use provider::{
    dir_size, dir_size_checked, dir_size_detail, dir_size_detail_checked, home_dir, impact,
    measure_dir, project_detail, stale_note, stale_note_for_age, tool_output_dir, walk_cancelled,
    CleanupAction, DirSize, OwnScanner, Provider, Scannable,
};
pub use python_cache::PythonCacheProvider;
pub use registry::ProviderRegistry;
pub use ruby::RubyProvider;
pub use rust_target::RustTargetProvider;
pub use tree::{walk_tree_rules, Match, TreeRule, TreeRuleSpec};
pub use xcode_data::XcodeDataProvider;

/// All providers in scan order.
pub fn all_providers() -> Vec<Scannable> {
    use std::sync::Arc;
    vec![
        Scannable::Tree(Arc::new(NodeModulesProvider)),
        Scannable::Tree(Arc::new(RustTargetProvider)),
        Scannable::Own(Arc::new(GoProvider)),
        Scannable::Own(Arc::new(DockerProvider)),
        Scannable::Own(Arc::new(DockerPrunedProvider)),
        Scannable::Own(Arc::new(GlobalCacheProvider)),
        Scannable::Own(Arc::new(NpmCacheProvider)),
        Scannable::Tree(Arc::new(PythonCacheProvider)),
        Scannable::Own(Arc::new(CargoRegistryProvider)),
        Scannable::Own(Arc::new(GradleCacheProvider)),
        Scannable::Own(Arc::new(XcodeDataProvider)),
        Scannable::Own(Arc::new(OpencodeFsProvider)),
        Scannable::Own(Arc::new(ClaudeProvider)),
        Scannable::Own(Arc::new(OpencodeDbProvider)),
        // Wave D3: four more rules on the shared walk.
        Scannable::Tree(Arc::new(GoVendorProvider)),
        Scannable::Tree(Arc::new(FlutterProvider)),
        Scannable::Tree(Arc::new(AndroidProvider)),
        Scannable::Tree(Arc::new(PodsProvider)),
        // Wave D4: the hazard.
        Scannable::Own(Arc::new(BrowserTempProvider::default())),
        // Wave D5.
        Scannable::Own(Arc::new(LlmModelsProvider)),
        Scannable::Own(Arc::new(RubyProvider)),
    ]
}
