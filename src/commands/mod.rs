//! Command implementations for the CLI
//!
//! Each submodule implements a single CLI command. All commands follow the same
//! pattern: they gather context, run the LLM via pipeline, parse the response,
//! and render output.

pub(crate) mod common;
mod config;
mod derive;
mod fix;
mod init;
mod review;

pub use config::cmd_config;
pub use derive::{DeriveOptions, DeriveType, cmd_derive};
pub use fix::{FixOptions, cmd_fix};
pub use init::{InitOptions, cmd_init};
pub use review::{ReviewOptions, cmd_review};
