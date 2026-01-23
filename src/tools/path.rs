//! Path utilities for tool execution
//!
//! Provides safe path resolution and JSON argument extraction helpers.

use std::path::{Path, PathBuf};

use super::ToolResult;

/// Extract a required string argument from JSON
pub(crate) fn get_required_str<'a>(
    args: &'a serde_json::Value,
    key: &str,
) -> Result<&'a str, ToolResult> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolResult::failure(format!("missing required argument: {key}")))
}

/// Extract an optional string argument from JSON
pub(crate) fn get_optional_str<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// Extract an optional bool argument from JSON with default
pub(crate) fn get_optional_bool(args: &serde_json::Value, key: &str, default: bool) -> bool {
    args.get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(default)
}

/// Resolve a user-provided path and validate it stays within the working directory.
///
/// This prevents path traversal attacks where `../` sequences could escape
/// the working directory to access unauthorized files.
///
/// # Errors
/// Returns an error message if:
/// - The working directory cannot be canonicalized
/// - The resolved path escapes the working directory
pub(crate) fn resolve_safe_path(working_dir: &Path, user_path: &str) -> Result<PathBuf, String> {
    let joined = working_dir.join(user_path);

    let canonical_working = working_dir
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize working directory: {e}"))?;

    // For existing files, canonicalize directly
    if joined.exists() {
        let canonical_target = joined
            .canonicalize()
            .map_err(|e| format!("failed to canonicalize path: {e}"))?;

        if !canonical_target.starts_with(&canonical_working) {
            return Err("path escapes working directory".to_string());
        }

        return Ok(canonical_target);
    }

    // For non-existent files (write operations), validate the parent directory
    // and construct the final path
    let parent = joined.parent().ok_or("invalid path: no parent directory")?;

    // If parent is empty (file in current dir), use working dir
    let canonical_parent = if parent.as_os_str().is_empty() {
        canonical_working.clone()
    } else if parent.exists() {
        parent
            .canonicalize()
            .map_err(|e| format!("failed to canonicalize parent directory: {e}"))?
    } else {
        // Parent doesn't exist - find deepest existing ancestor
        let mut ancestor = parent.to_path_buf();
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or("no valid ancestor directory")?
                .to_path_buf();
        }
        let canonical_ancestor = ancestor
            .canonicalize()
            .map_err(|e| format!("failed to canonicalize ancestor: {e}"))?;

        if !canonical_ancestor.starts_with(&canonical_working) {
            return Err("path escapes working directory".to_string());
        }

        // Return the original joined path since we'll create parent dirs
        return Ok(joined);
    };

    if !canonical_parent.starts_with(&canonical_working) {
        return Err("path escapes working directory".to_string());
    }

    let file_name = joined.file_name().ok_or("invalid path: no filename")?;

    Ok(canonical_parent.join(file_name))
}
