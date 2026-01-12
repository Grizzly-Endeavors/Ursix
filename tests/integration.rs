//! Integration tests for rust-code CLI

#![allow(clippy::unwrap_used)]
#![allow(deprecated)] // cargo_bin deprecation - revisit when assert_cmd stabilizes new API

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;
use predicates::prelude::*;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn cli_shows_version() -> TestResult {
    Command::cargo_bin("rust-code")?
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
    Ok(())
}

#[test]
fn cli_shows_help() -> TestResult {
    Command::cargo_bin("rust-code")?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("extensible agentic CLI"));
    Ok(())
}

#[test]
fn cli_accepts_model_flag() -> TestResult {
    // Verify --model flag is accepted by checking help output still works after it
    // (clap will error if an unknown flag is passed before --help)
    Command::cargo_bin("rust-code")?
        .args(["--model", "test-model", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("extensible agentic CLI"));
    Ok(())
}
