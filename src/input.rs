//! Input source handling for the CLI
//!
//! This module provides functions for reading input from stdin and files,
//! supporting Unix-idiomatic stdin auto-detection.

use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::{Context, Result};

/// Read content from stdin
pub fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .context("failed to read from stdin")?;
    Ok(buffer)
}

/// Check if stdin is piped (not a terminal)
pub fn stdin_is_piped() -> bool {
    use std::io::IsTerminal;
    !std::io::stdin().is_terminal()
}

/// Try to read stdin if piped, returning None if interactive or empty
pub fn try_read_piped_stdin() -> Option<String> {
    if !stdin_is_piped() {
        return None;
    }
    match read_stdin() {
        Ok(content) if !content.trim().is_empty() => Some(content),
        Ok(_) => {
            tracing::debug!("stdin is piped but empty, ignoring");
            None
        }
        Err(e) => {
            // Ensure user sees this error - don't silently ignore piped input failures
            eprintln!("warning: failed to read piped stdin: {e}");
            None
        }
    }
}

/// Read content from a file or stdin (if path is "-")
pub async fn read_from_source(path: &PathBuf) -> Result<String> {
    if path.as_os_str() == "-" {
        read_stdin()
    } else {
        tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read from {}", path.display()))
    }
}
