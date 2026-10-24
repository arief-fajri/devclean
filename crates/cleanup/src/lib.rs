//! Cleanup engine: `preview` (re-measure, never trust stale sizes) then
//! `execute` behind the safety validator.
//!
//! Safety rules enforced here:
//! - `RemoveDir` actions go through [`SafetyValidator::validate_path`].
//! - `RunCommand` actions never touch a path: only fixed
//!   `(program, args)` pairs declared by providers may run (G-SAFE-02), and
//!   never via a shell.

pub mod engine;
pub mod history;
pub mod retention;
pub mod sql;
pub mod trash;

pub use engine::{CleanupEngine, CleanupPolicy, CleanupProgressFn, Refusal};
pub use history::{append_history_entry, read_history, read_history_runs};
pub use retention::{prune, reclaimable_bytes};
pub use sql::run_sql;
pub use trash::move_to_trash;
