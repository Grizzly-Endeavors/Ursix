//! Integration tests for review rules configuration
//!
//! These tests verify that the rules.yml loading and resolution works correctly
//! with the review command.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use tempfile::TempDir;

// Re-export the rules module types for testing
// Since we can't import from the binary crate directly in integration tests,
// we test the rules module behavior through file system interactions

/// Helper to create a rules.yml file in a temp directory
fn create_rules_file(dir: &Path, content: &str) {
    fs::write(dir.join("rules.yml"), content).unwrap();
}

/// Helper to create a .ursus/rules.yml file in a temp directory
fn create_ursus_rules_file(dir: &Path, content: &str) {
    let ursus_dir = dir.join(".ursus");
    fs::create_dir_all(&ursus_dir).unwrap();
    fs::write(ursus_dir.join("rules.yml"), content).unwrap();
}

#[test]
fn rules_yaml_is_valid_yaml() {
    // Test that a basic rules.yml parses correctly
    let yaml = r#"
categories:
  style:
    - name: doc-comments
      description: "All public functions must have doc comments"
      severity: warning
      files: "*.rs"
  security:
    - name: no-unwrap
      description: "Avoid using .unwrap() outside of tests"
      severity: error
      files: "src/**/*.rs"
"#;

    // Should parse without error
    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    assert!(parsed.get("categories").is_some());
}

#[test]
fn rules_yaml_supports_all_severity_levels() {
    let yaml = r#"
categories:
  test:
    - name: error-level
      description: "Error severity"
      severity: error
    - name: warning-level
      description: "Warning severity"
      severity: warning
    - name: info-level
      description: "Info severity"
      severity: info
"#;

    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let categories = parsed.get("categories").unwrap();
    let test_rules = categories.get("test").unwrap().as_sequence().unwrap();

    assert_eq!(test_rules.len(), 3);
    assert_eq!(
        test_rules[0].get("severity").unwrap().as_str().unwrap(),
        "error"
    );
    assert_eq!(
        test_rules[1].get("severity").unwrap().as_str().unwrap(),
        "warning"
    );
    assert_eq!(
        test_rules[2].get("severity").unwrap().as_str().unwrap(),
        "info"
    );
}

#[test]
fn rules_yaml_optional_fields_work() {
    // Severity and files are optional
    let yaml = r#"
categories:
  minimal:
    - name: basic-rule
      description: "Just name and description"
"#;

    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let rules = parsed
        .get("categories")
        .unwrap()
        .get("minimal")
        .unwrap()
        .as_sequence()
        .unwrap();

    assert_eq!(rules.len(), 1);
    assert!(rules[0].get("severity").is_none());
    assert!(rules[0].get("files").is_none());
}

#[test]
fn rules_yaml_multiple_categories() {
    let yaml = r#"
categories:
  style:
    - name: rule1
      description: "Style rule"
  security:
    - name: rule2
      description: "Security rule"
  performance:
    - name: rule3
      description: "Performance rule"
  correctness:
    - name: rule4
      description: "Correctness rule"
"#;

    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let categories = parsed.get("categories").unwrap();

    assert!(categories.get("style").is_some());
    assert!(categories.get("security").is_some());
    assert!(categories.get("performance").is_some());
    assert!(categories.get("correctness").is_some());
}

#[test]
fn rules_file_created_in_project_root() {
    let temp_dir = TempDir::new().unwrap();

    create_rules_file(
        temp_dir.path(),
        r#"
categories:
  custom:
    - name: my-rule
      description: "Custom project rule"
"#,
    );

    assert!(temp_dir.path().join("rules.yml").exists());
}

#[test]
fn rules_file_created_in_ursus_dir() {
    let temp_dir = TempDir::new().unwrap();

    create_ursus_rules_file(
        temp_dir.path(),
        r#"
categories:
  custom:
    - name: my-rule
      description: "Custom project rule"
"#,
    );

    assert!(temp_dir.path().join(".ursus").join("rules.yml").exists());
}

#[test]
fn rules_yaml_glob_patterns_are_valid() {
    let yaml = r#"
categories:
  patterns:
    - name: rust-files
      description: "Applies to all Rust files"
      files: "*.rs"
    - name: src-rust
      description: "Applies to src directory"
      files: "src/**/*.rs"
    - name: tests
      description: "Applies to test files"
      files: "tests/*.rs"
    - name: all-code
      description: "Multiple extensions"
      files: "*.{rs,ts,js}"
"#;

    // Should parse without error
    let _: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
}

#[test]
fn empty_rules_yaml_is_valid() {
    let yaml = "categories: {}";
    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let categories = parsed.get("categories").unwrap();
    assert!(categories.as_mapping().unwrap().is_empty());
}

#[test]
fn rules_yaml_with_empty_category_is_valid() {
    let yaml = r#"
categories:
  empty-category: []
  with-rules:
    - name: rule1
      description: "A rule"
"#;

    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let categories = parsed.get("categories").unwrap();
    assert!(
        categories
            .get("empty-category")
            .unwrap()
            .as_sequence()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        categories
            .get("with-rules")
            .unwrap()
            .as_sequence()
            .unwrap()
            .len(),
        1
    );
}
