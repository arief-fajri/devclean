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

pub use engine::CleanupEngine;
pub use history::{append_history_entry, read_history};
