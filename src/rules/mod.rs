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
}
