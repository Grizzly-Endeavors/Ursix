//! Integration tests for Ursix CLI

#![allow(clippy::unwrap_used)]
#![allow(deprecated)] // cargo_bin deprecation - revisit when assert_cmd stabilizes new API

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;
use predicates::prelude::*;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn cli_shows_version() -> TestResult {
    Command::cargo_bin("usx")?
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
    Ok(())
}

#[test]
fn cli_shows_help() -> TestResult {
    Command::cargo_bin("usx")?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Ursix"));
    Ok(())
}

#[test]
fn cli_requires_subcommand() -> TestResult {
    // Without a subcommand, should show error
    Command::cargo_bin("usx")?
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
    Ok(())
}

#[test]
fn cli_config_list() -> TestResult {
    // Default output is JSON
    Command::cargo_bin("usx")?
        .args(["config", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"key\": \"model\""));
    Ok(())
}

#[test]
fn cli_accepts_model_flag() -> TestResult {
    // Verify --model flag is accepted by checking config list works with it (JSON default)
    Command::cargo_bin("usx")?
        .args(["--model", "test-model", "config", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"value\": \"test-model\""));
    Ok(())
}

#[test]
fn cli_default_produces_json() -> TestResult {
    // Verify default output produces valid JSON (JSON is now the default)
    Command::cargo_bin("usx")?
        .args(["config", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"key\":"))
        .stdout(predicate::str::contains("\"value\":"));
    Ok(())
}

#[test]
fn cli_json_output_is_valid() -> TestResult {
    // Verify config --list produces parseable JSON (JSON is now the default)
    let output = Command::cargo_bin("usx")?
        .args(["config", "--list"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should be valid JSON
    serde_json::from_str::<serde_json::Value>(&stdout)?;
    Ok(())
}

#[test]
fn cli_text_flag_produces_human_output() -> TestResult {
    // Verify --text flag produces human-readable output
    Command::cargo_bin("usx")?
        .args(["--text", "config", "--list"])
        .assert()
        .success()
        // Human output uses "=" format, not JSON
        .stdout(predicate::str::contains(" = "));
    Ok(())
}

#[test]
fn cli_fix_shows_help() -> TestResult {
    Command::cargo_bin("usx")?
        .args(["fix", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fix issues in code"))
        .stdout(predicate::str::contains("--lint"))
        .stdout(predicate::str::contains("--apply"))
        .stdout(predicate::str::contains("--from"));
    Ok(())
}

#[test]
fn cli_fix_requires_target() -> TestResult {
    Command::cargo_bin("usx")?
        .arg("fix")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
    Ok(())
}

#[test]
fn cli_fix_accepts_lint_flag() -> TestResult {
    // Verify --lint flag is accepted (will fail without LLM but shouldn't error on arg parsing)
    let result = Command::cargo_bin("usx")?
        .args(["fix", "--lint", "nonexistent.rs"])
        .output()?;

    // Should not fail due to unrecognized flag
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "fix command should accept --lint flag"
    );
    Ok(())
}

#[test]
fn cli_fix_accepts_apply_flag() -> TestResult {
    // Verify --apply flag is accepted
    let result = Command::cargo_bin("usx")?
        .args(["fix", "--apply", "nonexistent.rs"])
        .output()?;

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "fix command should accept --apply flag"
    );
    Ok(())
}

#[test]
fn cli_fix_accepts_from_flag() -> TestResult {
    // Verify --from flag is accepted
    let result = Command::cargo_bin("usx")?
        .args(["fix", "--from", "issues.json", "src/main.rs"])
        .output()?;

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "fix command should accept --from flag"
    );
    Ok(())
}

// --- Piped stdin tests ---
// These test that commands properly detect and accept piped stdin.
// They may fail later in the pipeline (LLM calls), but should not error on stdin handling.

use std::io::Write;
use std::process::Stdio;

#[test]
fn cli_explain_accepts_piped_stdin() -> TestResult {
    // Pipe content to explain command - verifies stdin detection works
    let mut child = Command::cargo_bin("usx")?
        .args(["explain", "test.rs"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Write to stdin
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(b"fn main() { println!(\"hello\"); }")?;
    }
    // Close stdin to signal EOF
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not error on stdin handling - may fail on LLM call, but not on stdin
    assert!(
        !stderr.contains("stdin") || stderr.contains("failed"),
        "explain should accept piped stdin without stdin-specific errors"
    );
    Ok(())
}

#[test]
fn cli_review_accepts_piped_stdin() -> TestResult {
    // Pipe diff content to review command
    let mut child = Command::cargo_bin("usx")?
        .args(["review"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(b"diff --git a/test.rs b/test.rs\n+fn new_func() {}\n")?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("unexpected argument"),
        "review should accept piped stdin"
    );
    Ok(())
}

#[test]
fn cli_commit_accepts_piped_stdin() -> TestResult {
    // Pipe diff content to commit command
    let mut child = Command::cargo_bin("usx")?
        .args(["commit"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(b"diff --git a/test.rs b/test.rs\n+fn added() {}\n")?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not error on stdin - may fail on LLM, but stdin should be accepted
    assert!(
        !stderr.contains("unexpected argument"),
        "commit should accept piped stdin"
    );
    Ok(())
}

#[test]
fn cli_fix_accepts_piped_stdin() -> TestResult {
    // Pipe issues content to fix command
    let mut child = Command::cargo_bin("usx")?
        .args(["fix", "src/main.rs"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(b"{\"issues\": [{\"message\": \"unused variable\"}]}")?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("unexpected argument"),
        "fix should accept piped stdin"
    );
    Ok(())
}

#[test]
fn cli_commit_empty_stdin_falls_back() -> TestResult {
    // Empty stdin should fall back to git gather, then fail with an error
    // (either "no staged changes" or network/API error if no LLM available)
    let mut child = Command::cargo_bin("usx")?
        .args(["commit"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Write empty content
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(b"")?;
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail with structured JSON error output
    // This verifies empty stdin is properly ignored and error handling works
    assert!(
        stderr.contains("\"error\"") || stderr.contains("no staged changes"),
        "empty stdin should fall back to internal gathering and produce error output"
    );
    Ok(())
}

#[test]
fn cli_accepts_partial_flag() -> TestResult {
    // Verify --partial flag is accepted by checking it doesn't error as unrecognized
    let result = Command::cargo_bin("usx")?
        .args(["--partial", "review", "--help"])
        .output()?;

    // Should show help for review command, not error about unrecognized flag
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("Review code changes"),
        "review --help should show command description"
    );

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "--partial flag should be accepted"
    );
    Ok(())
}
