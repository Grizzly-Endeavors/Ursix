//! Review rules configuration for customizable code review checks.
//!
//! Rules are natural language descriptions organized by category that tell the LLM
//! what to check during code review. Rules can be loaded from:
//! - Global config: `~/.config/ursix/rules.yml`
//! - Project config: `.ursix/rules.yml` or `rules.yml`
//!
//! Project rules extend global rules (same category adds rules, doesn't replace).

mod defaults;
mod loader;
mod resolver;

use std::collections::HashMap;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Severity level for a review rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Severity {
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
pub(crate) struct Rule {
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
pub(crate) struct RulesConfig {
    /// Rules organized by category (e.g., "style", "security", "performance")
    #[serde(default)]
    pub categories: HashMap<String, Vec<Rule>>,
}

impl RulesConfig {
    /// Returns the built-in default rules.
    #[must_use]
    pub(crate) fn defaults() -> Self {
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

    /// Load rules from global and project config files, merging them.
    ///
    /// Loading order:
    /// 1. Global rules from `~/.config/ursix/rules.yml`
    /// 2. Project rules from `.ursix/rules.yml` or `rules.yml`
    ///
    /// Project rules extend global rules (same category adds rules).
    /// If no rules files are found, returns an error directing the user to run `usx init`.
    ///
    /// # Errors
    /// Returns error if a rules file exists but cannot be parsed.
    pub(crate) fn load(working_dir: &std::path::Path) -> anyhow::Result<Self> {
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

        // Error if no rules found - require explicit rules file
        if !found_any {
            return Err(anyhow::anyhow!(
                "no rules.yml found; run 'usx init' to create starter rules, or create .ursix/rules.yml manually"
            ));
        }

        Ok(config)
    }

    /// Load rules from a specific YAML file.
    ///
    /// # Errors
    /// Returns error if the file cannot be read or parsed.
    fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read rules file: {}", path.display()))?;
        serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse rules file: {}", path.display()))
    }

    /// Merge another config into this one.
    /// Rules from the other config are appended to existing categories.
    pub(crate) fn merge(&mut self, other: Self) {
        for (category, rules) in other.categories {
            self.categories.entry(category).or_default().extend(rules);
        }
    }

    /// Resolve rules based on selected categories and file patterns.
    ///
    /// # Arguments
    /// * `checks` - Category names to include (empty means all categories)
    /// * `files` - File paths being reviewed (for pattern matching)
    #[must_use]
    pub(crate) fn resolve(&self, checks: &[String], files: &[std::path::PathBuf]) -> ResolvedRules {
        let filtered = self.filter_rules(checks, files);

        let mut resolved: Vec<ResolvedRule> =
            filtered.into_iter().flat_map(|(_, rules)| rules).collect();

        // Sort by category then name for consistent output
        resolved.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));

        ResolvedRules { rules: resolved }
    }

    /// Resolve rules grouped by category.
    ///
    /// Unlike `resolve()` which flattens all rules, this preserves category
    /// grouping for per-category LLM calls when `--checks` is specified.
    ///
    /// # Arguments
    /// * `checks` - Category names to include (empty means all categories)
    /// * `files` - File paths being reviewed (for pattern matching)
    #[must_use]
    pub(crate) fn resolve_by_category(
        &self,
        checks: &[String],
        files: &[std::path::PathBuf],
    ) -> CategoryResolvedRules {
        let mut category_rules = CategoryResolvedRules::default();
        let available_categories: Vec<&str> = self.categories.keys().map(String::as_str).collect();

        // Warn about checks that don't match any known category
        for check in checks {
            if !available_categories
                .iter()
                .any(|c| c.eq_ignore_ascii_case(check))
            {
                eprintln!(
                    "warning: no category named '{check}' found (available: {})",
                    available_categories.join(", ")
                );
            }
        }

        let filtered = self.filter_rules(checks, files);

        // Collect matched category names before consuming filtered
        let matched_categories: Vec<String> = filtered.iter().map(|(cat, _)| cat.clone()).collect();

        for (category, mut rules) in filtered {
            // Sort by name for consistent output within category
            rules.sort_by(|a, b| a.name.cmp(&b.name));
            category_rules.insert(category, ResolvedRules { rules });
        }

        // Warn about explicitly requested categories that had no rules matching files
        for check in checks {
            let check_matched = matched_categories
                .iter()
                .any(|c| c.eq_ignore_ascii_case(check));
            let category_exists = available_categories
                .iter()
                .any(|c| c.eq_ignore_ascii_case(check));

            if category_exists && !check_matched {
                // Find the actual category name for the warning message
                if let Some(category) = self
                    .categories
                    .keys()
                    .find(|c| c.eq_ignore_ascii_case(check))
                {
                    eprintln!(
                        "warning: category '{category}' has no rules matching the specified files"
                    );
                }
            }
        }

        category_rules
    }

    /// Filter rules by category and file patterns.
    ///
    /// Returns matching rules grouped by category name. Categories with no
    /// matching rules are omitted from the result.
    fn filter_rules(
        &self,
        checks: &[String],
        files: &[std::path::PathBuf],
    ) -> Vec<(String, Vec<ResolvedRule>)> {
        let mut result = Vec::new();

        for (category, rules) in &self.categories {
            // Skip if checks are specified and this category isn't in the list
            if !Self::category_matches_checks(category, checks) {
                continue;
            }

            let resolved: Vec<ResolvedRule> = rules
                .iter()
                .filter(|rule| Self::rule_matches_files(rule, files))
                .map(|rule| Self::resolve_rule(category, rule))
                .collect();

            if !resolved.is_empty() {
                result.push((category.clone(), resolved));
            }
        }

        result
    }

    /// Check if a category should be included based on the checks filter.
    fn category_matches_checks(category: &str, checks: &[String]) -> bool {
        checks.is_empty() || checks.iter().any(|c| c.eq_ignore_ascii_case(category))
    }

    /// Check if a rule applies to any of the given files.
    fn rule_matches_files(rule: &Rule, files: &[std::path::PathBuf]) -> bool {
        match &rule.files {
            Some(pattern) if !files.is_empty() => Self::matches_any_file(pattern, files),
            _ => true,
        }
    }

    /// Convert a `Rule` to a `ResolvedRule` with the given category.
    fn resolve_rule(category: &str, rule: &Rule) -> ResolvedRule {
        ResolvedRule {
            category: category.to_string(),
            name: rule.name.clone(),
            description: rule.description.clone(),
            severity: rule.severity,
        }
    }

    /// Check if a glob pattern matches any of the given files.
    fn matches_any_file(pattern: &str, files: &[std::path::PathBuf]) -> bool {
        let Ok(glob_pattern) = glob::Pattern::new(pattern) else {
            // Invalid pattern: warn and skip this rule rather than applying to everything
            eprintln!("warning: invalid glob pattern '{pattern}', skipping rule");
            return false;
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
pub(crate) struct ResolvedRule {
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
pub(crate) struct ResolvedRules {
    /// The resolved rules
    pub rules: Vec<ResolvedRule>,
}

impl ResolvedRules {
    /// Format rules as a markdown section for LLM prompt injection.
    #[must_use]
    pub(crate) fn to_prompt_section(&self) -> String {
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
                writeln!(output, "\n### {}", capitalize(category)).ok();
                for rule in rules {
                    writeln!(
                        output,
                        "- **{}** [{}]: {}",
                        rule.name, rule.severity, rule.description
                    )
                    .ok();
                }
            }
        }

        output
    }

    /// Format rules for a single category as a markdown section.
    #[must_use]
    pub(crate) fn to_category_prompt_section(&self, category: &str) -> String {
        use std::fmt::Write;

        if self.rules.is_empty() {
            return String::new();
        }

        let mut output = format!(
            "\n## Review Rules - {}\n\nApply these {} rules during your review:\n\n",
            capitalize(category),
            category
        );

        for rule in &self.rules {
            writeln!(
                output,
                "- **{}** [{}]: {}",
                rule.name, rule.severity, rule.description
            )
            .ok();
        }

        output
    }

    /// Returns true if there are no resolved rules.
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Returns the number of rules.
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.rules.len()
    }
}

/// Collection of resolved rules organized by category.
///
/// Used when `--checks` is specified to enable per-category LLM calls.
#[derive(Debug, Clone, Default)]
pub(crate) struct CategoryResolvedRules {
    /// Rules organized by category name
    categories: HashMap<String, ResolvedRules>,
}

impl CategoryResolvedRules {
    /// Returns true if there are no categories with rules.
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.categories.is_empty() || self.categories.values().all(ResolvedRules::is_empty)
    }

    /// Returns the number of categories.
    #[must_use]
    pub(crate) fn category_count(&self) -> usize {
        self.categories.len()
    }

    /// Iterate over categories and their rules.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &ResolvedRules)> {
        self.categories.iter()
    }

    /// Flatten all categories into a single `ResolvedRules`.
    #[must_use]
    pub(crate) fn flatten(&self) -> ResolvedRules {
        let mut all_rules = Vec::new();
        for rules in self.categories.values() {
            all_rules.extend(rules.rules.clone());
        }
        // Sort by category then name for consistent output
        all_rules.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
        ResolvedRules { rules: all_rules }
    }

    /// Insert rules for a category.
    pub(crate) fn insert(&mut self, category: String, rules: ResolvedRules) {
        self.categories.insert(category, rules);
    }

    /// Get rules for a specific category.
    #[must_use]
    pub(crate) fn get(&self, category: &str) -> Option<&ResolvedRules> {
        self.categories.get(category)
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
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
#[expect(clippy::panic, reason = "test code uses panic for clarity")]
mod tests {
    use super::*;

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

        let Some(style_rules) = config.categories.get("style") else {
            panic!("expected 'style' category")
        };
        assert_eq!(style_rules.len(), 1);
        assert_eq!(
            style_rules.get(0).map(|r| &r.name),
            Some(&"doc-comments".to_string())
        );
        assert_eq!(
            style_rules.get(0).map(|r| r.severity),
            Some(Severity::Warning)
        );
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
        let Some(rules) = config.categories.get("test") else {
            panic!("expected 'test' category")
        };
        assert_eq!(rules.get(0).map(|r| r.severity), Some(Severity::Warning)); // default
        assert!(rules.get(0).and_then(|r| r.files.as_ref()).is_none());
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
    fn test_to_category_prompt_section() {
        let resolved = ResolvedRules {
            rules: vec![
                ResolvedRule {
                    category: "security".to_string(),
                    name: "no-unwrap".to_string(),
                    description: "Avoid unwrap".to_string(),
                    severity: Severity::Error,
                },
                ResolvedRule {
                    category: "security".to_string(),
                    name: "no-secrets".to_string(),
                    description: "No hardcoded secrets".to_string(),
                    severity: Severity::Error,
                },
            ],
        };

        let section = resolved.to_category_prompt_section("security");
        assert!(section.contains("## Review Rules - Security"));
        assert!(section.contains("security rules"));
        assert!(section.contains("**no-unwrap** [error]"));
        assert!(section.contains("**no-secrets** [error]"));
    }

    #[test]
    fn test_resolved_rules_len() {
        let resolved = ResolvedRules {
            rules: vec![
                ResolvedRule {
                    category: "test".to_string(),
                    name: "rule1".to_string(),
                    description: "desc".to_string(),
                    severity: Severity::Warning,
                },
                ResolvedRule {
                    category: "test".to_string(),
                    name: "rule2".to_string(),
                    description: "desc".to_string(),
                    severity: Severity::Warning,
                },
            ],
        };
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn test_category_resolved_rules_empty() {
        let cat_rules = CategoryResolvedRules::default();
        assert!(cat_rules.is_empty());
        assert_eq!(cat_rules.category_count(), 0);
    }

    #[test]
    fn test_category_resolved_rules_insert_and_get() {
        let mut cat_rules = CategoryResolvedRules::default();

        let security_rules = ResolvedRules {
            rules: vec![ResolvedRule {
                category: "security".to_string(),
                name: "no-unwrap".to_string(),
                description: "Avoid unwrap".to_string(),
                severity: Severity::Error,
            }],
        };

        cat_rules.insert("security".to_string(), security_rules);

        assert!(!cat_rules.is_empty());
        assert_eq!(cat_rules.category_count(), 1);
        assert!(cat_rules.get("security").is_some());
        assert!(cat_rules.get("style").is_none());
    }

    #[test]
    fn test_category_resolved_rules_iter() {
        let mut cat_rules = CategoryResolvedRules::default();

        cat_rules.insert(
            "security".to_string(),
            ResolvedRules {
                rules: vec![ResolvedRule {
                    category: "security".to_string(),
                    name: "sec-rule".to_string(),
                    description: "desc".to_string(),
                    severity: Severity::Error,
                }],
            },
        );

        cat_rules.insert(
            "style".to_string(),
            ResolvedRules {
                rules: vec![ResolvedRule {
                    category: "style".to_string(),
                    name: "style-rule".to_string(),
                    description: "desc".to_string(),
                    severity: Severity::Warning,
                }],
            },
        );

        let categories: Vec<_> = cat_rules.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(categories.len(), 2);
        assert!(categories.contains(&"security".to_string()));
        assert!(categories.contains(&"style".to_string()));
    }

    #[test]
    fn test_category_resolved_rules_flatten() {
        let mut cat_rules = CategoryResolvedRules::default();

        cat_rules.insert(
            "security".to_string(),
            ResolvedRules {
                rules: vec![ResolvedRule {
                    category: "security".to_string(),
                    name: "sec-rule".to_string(),
                    description: "desc".to_string(),
                    severity: Severity::Error,
                }],
            },
        );

        cat_rules.insert(
            "style".to_string(),
            ResolvedRules {
                rules: vec![ResolvedRule {
                    category: "style".to_string(),
                    name: "style-rule".to_string(),
                    description: "desc".to_string(),
                    severity: Severity::Warning,
                }],
            },
        );

        let flattened = cat_rules.flatten();
        assert_eq!(flattened.len(), 2);
        // Should be sorted by category then name
        assert_eq!(
            flattened.rules.get(0).map(|r| &r.category),
            Some(&"security".to_string())
        );
        assert_eq!(
            flattened.rules.get(1).map(|r| &r.category),
            Some(&"style".to_string())
        );
    }
}
