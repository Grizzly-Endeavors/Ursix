//! Review rules configuration for customizable code review checks.
//!
//! Rules are natural language descriptions organized by category that tell the LLM
//! what to check during code review. Rules can be loaded from:
//! - Global config: `~/.config/ursix/rules.yml`
//! - Project config: `.ursix/rules.yml` or `rules.yml`
//!
//! Project rules extend global rules (same category adds rules, doesn't replace).

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Severity level for a review rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Critical issue that must be fixed
    Error,
    /// Potential problem that should be addressed
    #[default]
    Warning,
    /// Suggestion for improvement
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Info => write!(f, "info"),
        }
    }
}

/// A single review rule definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Short identifier for the rule
    pub name: String,
    /// Natural language description of what to check
    pub description: String,
    /// Severity level when this rule is violated
    #[serde(default)]
    pub severity: Severity,
    /// Optional glob pattern for files this rule applies to
    #[serde(default)]
    pub files: Option<String>,
}

/// Configuration containing all review rules organized by category.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RulesConfig {
    /// Rules organized by category (e.g., "style", "security", "performance")
    #[serde(default)]
    pub categories: HashMap<String, Vec<Rule>>,
}

impl RulesConfig {
    /// Load rules from global and project config files, merging them.
    ///
    /// Loading order:
    /// 1. Global rules from `~/.config/ursix/rules.yml`
    /// 2. Project rules from `.ursix/rules.yml` or `rules.yml`
    ///
    /// Project rules extend global rules (same category adds rules).
    /// If no rules are found, returns default built-in rules.
    ///
    /// # Errors
    /// Returns error if a rules file exists but cannot be parsed.
    pub fn load(working_dir: &Path) -> Result<Self> {
        let mut config = Self::default();
        let mut found_any = false;

        // Load global rules
        if let Some(global_path) = dirs::config_dir() {
            let global_rules_path = global_path.join("ursix").join("rules.yml");
            if global_rules_path.exists() {
                let global_config = Self::load_from_file(&global_rules_path)?;
                config.merge(global_config);
                found_any = true;
            }
        }

        // Load project rules (check .ursix/rules.yml first, then rules.yml)
        let project_paths = [
            working_dir.join(".ursix").join("rules.yml"),
            working_dir.join("rules.yml"),
        ];

        for project_path in &project_paths {
            if project_path.exists() {
                let project_config = Self::load_from_file(project_path)?;
                config.merge(project_config);
                found_any = true;
                break; // Only load one project config
            }
        }

        // Use defaults if no rules found
        if !found_any {
            return Ok(Self::defaults());
        }

        Ok(config)
    }

    /// Load rules from a specific YAML file.
    ///
    /// # Errors
    /// Returns error if the file cannot be read or parsed.
    fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read rules file: {}", path.display()))?;
        serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse rules file: {}", path.display()))
    }

    /// Merge another config into this one.
    /// Rules from the other config are appended to existing categories.
    fn merge(&mut self, other: Self) {
        for (category, rules) in other.categories {
            self.categories.entry(category).or_default().extend(rules);
        }
    }

    /// Returns the built-in default rules.
    #[must_use]
    pub fn defaults() -> Self {
        let mut categories = HashMap::new();

        // Style rules
        categories.insert(
            "style".to_string(),
            vec![
                Rule {
                    name: "doc-comments".to_string(),
                    description: "Public functions and types should have doc comments".to_string(),
                    severity: Severity::Warning,
                    files: Some("*.rs".to_string()),
                },
                Rule {
                    name: "naming-conventions".to_string(),
                    description: "Follow Rust naming conventions (snake_case for functions, CamelCase for types)".to_string(),
                    severity: Severity::Warning,
                    files: Some("*.rs".to_string()),
                },
            ],
        );

        // Security rules
        categories.insert(
            "security".to_string(),
            vec![
                Rule {
                    name: "no-unwrap".to_string(),
                    description:
                        "Avoid using .unwrap() outside of tests - use proper error handling"
                            .to_string(),
                    severity: Severity::Error,
                    files: Some("src/**/*.rs".to_string()),
                },
                Rule {
                    name: "no-hardcoded-secrets".to_string(),
                    description: "Do not hardcode secrets, API keys, or passwords".to_string(),
                    severity: Severity::Error,
                    files: None,
                },
                Rule {
                    name: "input-validation".to_string(),
                    description: "Validate and sanitize external input before use".to_string(),
                    severity: Severity::Error,
                    files: None,
                },
            ],
        );

        // Performance rules
        categories.insert(
            "performance".to_string(),
            vec![
                Rule {
                    name: "avoid-clone".to_string(),
                    description: "Avoid unnecessary .clone() calls - prefer borrowing when possible".to_string(),
                    severity: Severity::Warning,
                    files: Some("*.rs".to_string()),
                },
                Rule {
                    name: "efficient-collections".to_string(),
                    description: "Use appropriate collection types (Vec vs HashSet vs HashMap) for the use case".to_string(),
                    severity: Severity::Info,
                    files: Some("*.rs".to_string()),
                },
            ],
        );

        // Correctness rules
        categories.insert(
            "correctness".to_string(),
            vec![
                Rule {
                    name: "error-handling".to_string(),
                    description: "Handle all error cases explicitly - don't ignore Results"
                        .to_string(),
                    severity: Severity::Error,
                    files: Some("*.rs".to_string()),
                },
                Rule {
                    name: "boundary-conditions".to_string(),
                    description:
                        "Check for boundary conditions (empty collections, zero values, overflow)"
                            .to_string(),
                    severity: Severity::Warning,
                    files: None,
                },
            ],
        );

        Self { categories }
    }

    /// Resolve rules based on selected categories and file patterns.
    ///
    /// # Arguments
    /// * `checks` - Category names to include (empty means all categories)
    /// * `files` - File paths being reviewed (for pattern matching)
    #[must_use]
    pub fn resolve(&self, checks: &[String], files: &[std::path::PathBuf]) -> ResolvedRules {
        let mut resolved = Vec::new();

        for (category, rules) in &self.categories {
            // Skip if checks are specified and this category isn't in the list
            if !checks.is_empty() && !checks.iter().any(|c| c.eq_ignore_ascii_case(category)) {
                continue;
            }

            for rule in rules {
                // Check if rule applies to any of the files
                if let Some(ref pattern) = rule.files
                    && !files.is_empty()
                    && !Self::matches_any_file(pattern, files)
                {
                    continue;
                }

                resolved.push(ResolvedRule {
                    category: category.clone(),
                    name: rule.name.clone(),
                    description: rule.description.clone(),
                    severity: rule.severity,
                });
            }
        }

        // Sort by category then name for consistent output
        resolved.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));

        ResolvedRules { rules: resolved }
    }

    /// Check if a glob pattern matches any of the given files.
    fn matches_any_file(pattern: &str, files: &[std::path::PathBuf]) -> bool {
        let Ok(glob_pattern) = glob::Pattern::new(pattern) else {
            return true; // Invalid pattern matches everything
        };

        files.iter().any(|file| {
            let path_str = file.to_string_lossy();
            glob_pattern.matches(&path_str)
                || file
                    .file_name()
                    .is_some_and(|name| glob_pattern.matches(&name.to_string_lossy()))
        })
    }
}

/// A rule that has been resolved for a specific review context.
#[derive(Debug, Clone)]
pub struct ResolvedRule {
    /// Category this rule belongs to
    pub category: String,
    /// Rule identifier
    pub name: String,
    /// What to check
    pub description: String,
    /// Severity when violated
    pub severity: Severity,
}

/// Collection of resolved rules ready for prompt injection.
#[derive(Debug, Clone, Default)]
pub struct ResolvedRules {
    /// The resolved rules
    pub rules: Vec<ResolvedRule>,
}

impl ResolvedRules {
    /// Format rules as a markdown section for LLM prompt injection.
    #[must_use]
    pub fn to_prompt_section(&self) -> String {
        if self.rules.is_empty() {
            return String::new();
        }

        let mut output =
            String::from("\n## Review Rules\n\nApply these rules during your review:\n");

        // Group rules by category for cleaner output
        let mut by_category: HashMap<&str, Vec<&ResolvedRule>> = HashMap::new();
        for rule in &self.rules {
            by_category.entry(&rule.category).or_default().push(rule);
        }

        // Sort categories for consistent output
        let mut categories: Vec<_> = by_category.keys().copied().collect();
        categories.sort_unstable();

        for category in categories {
            if let Some(rules) = by_category.get(category) {
                use std::fmt::Write;
                let _ = writeln!(output, "\n### {}", capitalize(category));
                for rule in rules {
                    let _ = writeln!(
                        output,
                        "- **{}** [{}]: {}",
                        rule.name, rule.severity, rule.description
                    );
                }
            }
        }

        output
    }

    /// Returns true if there are no resolved rules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Capitalize the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_severity_display() {
        assert_eq!(Severity::Error.to_string(), "error");
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Info.to_string(), "info");
    }

    #[test]
    fn test_severity_default() {
        assert_eq!(Severity::default(), Severity::Warning);
    }

    #[test]
    fn test_parse_yaml_basic() {
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
        let config: RulesConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.categories.len(), 2);
        assert!(config.categories.contains_key("style"));
        assert!(config.categories.contains_key("security"));

        let style_rules = config.categories.get("style").unwrap();
        assert_eq!(style_rules.len(), 1);
        assert_eq!(style_rules[0].name, "doc-comments");
        assert_eq!(style_rules[0].severity, Severity::Warning);
    }

    #[test]
    fn test_parse_yaml_missing_optional_fields() {
        let yaml = r#"
categories:
  test:
    - name: basic-rule
      description: "A basic rule"
"#;
        let config: RulesConfig = serde_yaml::from_str(yaml).unwrap();
        let rules = config.categories.get("test").unwrap();
        assert_eq!(rules[0].severity, Severity::Warning); // default
        assert!(rules[0].files.is_none());
    }

    #[test]
    fn test_merge_configs() {
        let mut config1 = RulesConfig::default();
        config1.categories.insert(
            "style".to_string(),
            vec![Rule {
                name: "rule1".to_string(),
                description: "Rule 1".to_string(),
                severity: Severity::Warning,
                files: None,
            }],
        );

        let mut config2 = RulesConfig::default();
        config2.categories.insert(
            "style".to_string(),
            vec![Rule {
                name: "rule2".to_string(),
                description: "Rule 2".to_string(),
                severity: Severity::Error,
                files: None,
            }],
        );
        config2.categories.insert(
            "security".to_string(),
            vec![Rule {
                name: "rule3".to_string(),
                description: "Rule 3".to_string(),
                severity: Severity::Error,
                files: None,
            }],
        );

        config1.merge(config2);

        assert_eq!(config1.categories.len(), 2);
        assert_eq!(config1.categories.get("style").unwrap().len(), 2);
        assert_eq!(config1.categories.get("security").unwrap().len(), 1);
    }

    #[test]
    fn test_resolve_all_categories() {
        let config = RulesConfig::defaults();
        let resolved = config.resolve(&[], &[]);
        assert!(!resolved.is_empty());
    }

    #[test]
    fn test_resolve_specific_categories() {
        let config = RulesConfig::defaults();
        let checks = vec!["security".to_string()];
        let resolved = config.resolve(&checks, &[]);

        assert!(!resolved.is_empty());
        for rule in &resolved.rules {
            assert_eq!(rule.category, "security");
        }
    }

    #[test]
    fn test_resolve_case_insensitive_categories() {
        let config = RulesConfig::defaults();
        let checks = vec!["SECURITY".to_string()];
        let resolved = config.resolve(&checks, &[]);

        assert!(!resolved.is_empty());
        for rule in &resolved.rules {
            assert_eq!(rule.category, "security");
        }
    }

    #[test]
    fn test_resolve_with_file_patterns() {
        let mut config = RulesConfig::default();
        config.categories.insert(
            "test".to_string(),
            vec![
                Rule {
                    name: "rs-only".to_string(),
                    description: "Rust files only".to_string(),
                    severity: Severity::Warning,
                    files: Some("*.rs".to_string()),
                },
                Rule {
                    name: "all-files".to_string(),
                    description: "All files".to_string(),
                    severity: Severity::Warning,
                    files: None,
                },
            ],
        );

        // With .rs files, both rules apply
        let files = vec![PathBuf::from("src/main.rs")];
        let resolved = config.resolve(&[], &files);
        assert_eq!(resolved.rules.len(), 2);

        // With .py files, only all-files rule applies
        let files = vec![PathBuf::from("script.py")];
        let resolved = config.resolve(&[], &files);
        assert_eq!(resolved.rules.len(), 1);
        assert_eq!(resolved.rules[0].name, "all-files");
    }

    #[test]
    fn test_defaults_not_empty() {
        let defaults = RulesConfig::defaults();
        assert!(!defaults.categories.is_empty());
        assert!(defaults.categories.contains_key("style"));
        assert!(defaults.categories.contains_key("security"));
        assert!(defaults.categories.contains_key("performance"));
        assert!(defaults.categories.contains_key("correctness"));
    }

    #[test]
    fn test_to_prompt_section_empty() {
        let resolved = ResolvedRules::default();
        assert!(resolved.to_prompt_section().is_empty());
    }

    #[test]
    fn test_to_prompt_section_format() {
        let resolved = ResolvedRules {
            rules: vec![
                ResolvedRule {
                    category: "security".to_string(),
                    name: "no-unwrap".to_string(),
                    description: "Avoid unwrap".to_string(),
                    severity: Severity::Error,
                },
                ResolvedRule {
                    category: "style".to_string(),
                    name: "docs".to_string(),
                    description: "Add docs".to_string(),
                    severity: Severity::Warning,
                },
            ],
        };

        let section = resolved.to_prompt_section();
        assert!(section.contains("## Review Rules"));
        assert!(section.contains("### Security"));
        assert!(section.contains("### Style"));
        assert!(section.contains("**no-unwrap** [error]"));
        assert!(section.contains("**docs** [warning]"));
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("hello"), "Hello");
        assert_eq!(capitalize("HELLO"), "HELLO");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("a"), "A");
    }

    #[test]
    fn test_load_uses_defaults_when_no_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = RulesConfig::load(temp_dir.path()).unwrap();
        // Should have default rules
        assert!(!config.categories.is_empty());
    }
}
