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
mod status;

pub(crate) use config::cmd_config;
pub(crate) use derive::{DeriveOptions, DeriveType, cmd_derive};
pub(crate) use fix::{FixOptions, cmd_fix};
pub(crate) use init::{InitOptions, cmd_init};
pub(crate) use review::{DiffDetection, ReviewOptions, cmd_review};
pub(crate) use status::{StatusOptions, cmd_status};
