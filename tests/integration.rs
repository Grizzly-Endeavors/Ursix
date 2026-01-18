//! Integration tests for Ursus.rs CLI

#![allow(clippy::unwrap_used)]
#![allow(deprecated)] // cargo_bin deprecation - revisit when assert_cmd stabilizes new API

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;
use predicates::prelude::*;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn cli_shows_version() -> TestResult {
    Command::cargo_bin("ur")?
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
    Ok(())
}

#[test]
fn cli_shows_help() -> TestResult {
    Command::cargo_bin("ur")?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Ursus.rs"));
    Ok(())
}

#[test]
fn cli_requires_subcommand() -> TestResult {
    // Without a subcommand, should show error
    Command::cargo_bin("ur")?
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
    Ok(())
}

#[test]
fn cli_ask_shows_help() -> TestResult {
    Command::cargo_bin("ur")?
        .args(["ask", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("General-purpose LLM query"));
    Ok(())
}

#[test]
fn cli_ask_requires_prompt() -> TestResult {
    // Ask without a prompt should fail
    Command::cargo_bin("ur")?
        .arg("ask")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
    Ok(())
}

#[test]
fn cli_config_list() -> TestResult {
    Command::cargo_bin("ur")?
        .args(["config", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("model = "));
    Ok(())
}

#[test]
fn cli_accepts_model_flag() -> TestResult {
    // Verify --model flag is accepted by checking config list works with it
    Command::cargo_bin("ur")?
        .args(["--model", "test-model", "config", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("model = test-model"));
    Ok(())
}

#[test]
fn cli_accepts_json_flag() -> TestResult {
    // Verify --json flag is accepted (doesn't affect config output yet, but should parse)
    Command::cargo_bin("ur")?
        .args(["--json", "config", "--list"])
        .assert()
        .success();
    Ok(())
}
