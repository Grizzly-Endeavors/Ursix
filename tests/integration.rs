//! Integration tests for Ursix CLI

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;
use predicates::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[cfg(test)]
mod tests {
    use super::*;

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
            .stdout(predicate::str::contains("[FILE]"))
            .stdout(predicate::str::contains("--context"))
            .stdout(predicate::str::contains("--retry"))
            .stdout(predicate::str::contains("--partial"));
        Ok(())
    }

    #[test]
    fn cli_fix_accepts_positional_file() -> TestResult {
        // Verify positional file argument is accepted
        let result = Command::cargo_bin("usx")?
            .args(["fix", "issues.json"])
            .output()?;

        let stderr = String::from_utf8_lossy(&result.stderr);
        // Will fail on file not found, but shouldn't error on arg parsing
        assert!(
            !stderr.contains("unexpected argument"),
            "fix command should accept positional file argument"
        );
        Ok(())
    }

    // --- Piped stdin tests ---
    // These test that commands properly accept piped stdin.
    // They may fail later in the pipeline (LLM calls), but should not error on stdin handling.

    #[test]
    fn cli_derive_accepts_piped_stdin() -> TestResult {
        // Pipe content to derive explanation command - verifies stdin detection works
        let mut child = Command::cargo_bin("usx")?
            .args(["derive", "explanation"])
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
            !stderr.contains("unexpected argument"),
            "derive should accept piped stdin without argument errors"
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
    fn cli_derive_commit_msg_accepts_piped_stdin() -> TestResult {
        // Pipe diff content to derive commit-msg command
        let mut child = Command::cargo_bin("usx")?
            .args(["derive", "commit-msg"])
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
            "derive commit-msg should accept piped stdin"
        );
        Ok(())
    }

    #[test]
    fn cli_fix_accepts_piped_stdin() -> TestResult {
        // Pipe JSON input to fix command (fix now expects structured JSON input)
        let mut child = Command::cargo_bin("usx")?
            .args(["fix"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Fix command expects JSON with issue, snippet, file, lines
        // This will fail validation (file doesn't exist) but tests stdin handling
        let json_input =
            r#"{"issue": "test", "snippet": "code", "file": "test.rs", "lines": [1, 1]}"#;
        if let Some(ref mut stdin) = child.stdin {
            stdin.write_all(json_input.as_bytes())?;
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
    fn cli_derive_empty_stdin_errors() -> TestResult {
        // Empty stdin should error with "input is empty" message
        let mut child = Command::cargo_bin("usx")?
            .args(["derive", "commit-msg"])
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

        // Should fail with empty input error
        assert!(
            stderr.contains("input is empty") || stderr.contains("provide content"),
            "empty stdin should produce 'input is empty' error"
        );
        Ok(())
    }

    #[test]
    fn cli_review_shows_diff_flag_in_help() -> TestResult {
        Command::cargo_bin("usx")?
            .args(["review", "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--diff"));
        Ok(())
    }

    #[test]
    fn cli_review_accepts_diff_flag() -> TestResult {
        // Pipe diff content with --diff flag - verifies flag is accepted
        let mut child = Command::cargo_bin("usx")?
            .args(["review", "--diff"])
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
            "review should accept --diff flag"
        );
        Ok(())
    }

    #[test]
    fn cli_derive_shows_help() -> TestResult {
        // Verify derive command shows help with available types
        let result = Command::cargo_bin("usx")?
            .args(["derive", "--help"])
            .output()?;

        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            stdout.contains("Derive content from input"),
            "derive --help should show command description"
        );
        assert!(
            stdout.contains("--chunk"),
            "derive should support --chunk flag"
        );
        Ok(())
    }
}
