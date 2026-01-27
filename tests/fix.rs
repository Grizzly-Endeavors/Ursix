//! Integration tests for the fix command

#![allow(clippy::unwrap_used)]
#![allow(deprecated)]

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;
use predicates::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Create a test file and return the directory and file name
fn setup_test_file(content: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.rs");
    std::fs::write(&file_path, content).unwrap();
    (dir, "test.rs".to_string())
}

// === Input Validation Tests ===

#[test]
fn fix_rejects_invalid_json() -> TestResult {
    let mut child = Command::cargo_bin("usx")?
        .args(["fix"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(b"not valid json")?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should fail with exit code 2 (UserError)
    assert!(!output.status.success());
    assert!(stdout.contains("error") || stdout.contains("failed to parse"));

    Ok(())
}

#[test]
fn fix_rejects_missing_fields() -> TestResult {
    let mut child = Command::cargo_bin("usx")?
        .args(["fix"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Missing 'lines' field
    let json = r#"{"issue": "test", "snippet": "code", "file": "test.rs"}"#;
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(json.as_bytes())?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;

    assert!(!output.status.success());

    Ok(())
}

#[test]
fn fix_rejects_nonexistent_file() -> TestResult {
    let dir = TempDir::new()?;

    let mut child = Command::cargo_bin("usx")?
        .args(["fix"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let json = r#"{"issue": "test", "snippet": "code", "file": "nonexistent.rs", "lines": [1, 1]}"#;
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(json.as_bytes())?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!output.status.success());
    assert!(stdout.contains("does not exist") || stdout.contains("error"));

    Ok(())
}

#[test]
fn fix_rejects_invalid_line_range() -> TestResult {
    let (dir, file_name) = setup_test_file("line 1\nline 2\nline 3");

    let mut child = Command::cargo_bin("usx")?
        .args(["fix"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Line range out of bounds
    let json = format!(
        r#"{{"issue": "test", "snippet": "line 1", "file": "{file_name}", "lines": [1, 100]}}"#
    );
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(json.as_bytes())?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!output.status.success());
    assert!(
        stdout.contains("out of bounds") || stdout.contains("exceeds") || stdout.contains("error")
    );

    Ok(())
}

#[test]
fn fix_rejects_snippet_mismatch() -> TestResult {
    let (dir, file_name) = setup_test_file("actual content");

    let mut child = Command::cargo_bin("usx")?
        .args(["fix"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let json = format!(
        r#"{{"issue": "test", "snippet": "wrong content", "file": "{file_name}", "lines": [1, 1]}}"#
    );
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(json.as_bytes())?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!output.status.success());
    assert!(
        stdout.contains("mismatch")
            || stdout.contains("does not match")
            || stdout.contains("error")
    );

    Ok(())
}

// === CLI Flag Tests ===

#[test]
fn fix_help_shows_all_flags() -> TestResult {
    Command::cargo_bin("usx")?
        .args(["fix", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--context"))
        .stdout(predicate::str::contains("--retry"))
        .stdout(predicate::str::contains("--partial"))
        .stdout(predicate::str::contains("--from"));

    Ok(())
}

#[test]
fn fix_accepts_context_flag() -> TestResult {
    // Just verify the flag is accepted (will fail on validation, that's fine)
    let result = Command::cargo_bin("usx")?
        .args(["fix", "--context", "5"])
        .output()?;

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "--context flag should be accepted"
    );

    Ok(())
}

#[test]
fn fix_accepts_retry_flag() -> TestResult {
    let result = Command::cargo_bin("usx")?
        .args(["fix", "--retry"])
        .output()?;

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "--retry flag should be accepted"
    );

    Ok(())
}

#[test]
fn fix_accepts_partial_flag() -> TestResult {
    let result = Command::cargo_bin("usx")?
        .args(["fix", "--partial"])
        .output()?;

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "--partial flag should be accepted"
    );

    Ok(())
}

// === Output Format Tests ===

#[test]
fn fix_error_output_is_valid_json() -> TestResult {
    let mut child = Command::cargo_bin("usx")?
        .args(["fix"]) // JSON is default
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(b"invalid json")?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Error output should be valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
    assert!(parsed.get("message").is_some() || parsed.get("error").is_some());

    Ok(())
}

#[test]
fn fix_error_text_output() -> TestResult {
    let mut child = Command::cargo_bin("usx")?
        .args(["--text", "fix"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(b"invalid json")?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Text output should not be JSON
    assert!(
        stdout.starts_with("error:"),
        "Text mode should show 'error:' prefix"
    );

    Ok(())
}

// === Dry-Run Tests ===

#[test]
fn fix_dry_run_succeeds_with_valid_input() -> TestResult {
    let (dir, file_name) = setup_test_file("let x = 1;");

    let mut child = Command::cargo_bin("usx")?
        .args(["--dry-run", "fix"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let json = format!(
        r#"{{"issue": "unused variable", "snippet": "let x = 1;", "file": "{file_name}", "lines": [1, 1]}}"#
    );
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(json.as_bytes())?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    // Dry-run should mention the file and lines
    assert!(stdout.contains("test.rs") || stdout.contains("lines"));

    Ok(())
}

#[test]
fn fix_dry_run_json_output() -> TestResult {
    let (dir, file_name) = setup_test_file("content");

    let mut child = Command::cargo_bin("usx")?
        .args(["--dry-run", "fix"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let json = format!(
        r#"{{"issue": "test", "snippet": "content", "file": "{file_name}", "lines": [1, 1]}}"#
    );
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(json.as_bytes())?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
    assert!(parsed.get("file").is_some());
    assert!(parsed.get("lines").is_some());

    Ok(())
}

// === From File Tests ===

#[test]
fn fix_reads_input_from_file() -> TestResult {
    let dir = TempDir::new()?;
    let code_file = dir.path().join("code.rs");
    let input_file = dir.path().join("input.json");

    std::fs::write(&code_file, "let x = 1;")?;

    let json = r#"{"issue": "test", "snippet": "let x = 1;", "file": "code.rs", "lines": [1, 1]}"#;
    std::fs::write(&input_file, json)?;

    // With --dry-run to avoid needing LLM
    let output = Command::cargo_bin("usx")?
        .args(["--dry-run", "fix", "--from", "input.json"])
        .current_dir(dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "Should read input from --from file"
    );

    Ok(())
}
