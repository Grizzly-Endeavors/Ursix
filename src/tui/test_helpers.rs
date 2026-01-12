//! Shared test utilities for TUI tests

use crate::config::Config;
use std::path::PathBuf;

/// Create a default Config for testing
pub fn default_config() -> Config {
    Config {
        model: "test".to_string(),
        ollama_url: "http://localhost:11434".to_string(),
        working_dir: PathBuf::from("/tmp"),
        max_turns: 10,
    }
}
