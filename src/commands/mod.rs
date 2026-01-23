//! Command implementations for the CLI
//!
//! Each submodule implements a single CLI command. All commands follow the same
//! pattern: they gather context, run the LLM (pipeline or agent mode), parse the
//! response, and render output.

mod commit;
mod config;
mod explain;
mod fix;
mod review;

pub use commit::cmd_commit;
pub use config::cmd_config;
pub use explain::cmd_explain;
pub use fix::cmd_fix;
pub use review::cmd_review;
