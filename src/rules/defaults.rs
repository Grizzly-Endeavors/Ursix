//! Default rules for code review.

use std::collections::HashMap;

use super::{Rule, RulesConfig, Severity};

impl RulesConfig {
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_not_empty() {
        let defaults = RulesConfig::defaults();
        assert!(!defaults.categories.is_empty());
        assert!(defaults.categories.contains_key("style"));
        assert!(defaults.categories.contains_key("security"));
        assert!(defaults.categories.contains_key("performance"));
        assert!(defaults.categories.contains_key("correctness"));
    }
}
