//! Rules resolution based on categories and file patterns.

use std::path::PathBuf;

use super::{CategoryResolvedRules, ResolvedRule, ResolvedRules, Rule, RulesConfig};

impl RulesConfig {
    /// Resolve rules based on selected categories and file patterns.
    ///
    /// # Arguments
    /// * `checks` - Category names to include (empty means all categories)
    /// * `files` - File paths being reviewed (for pattern matching)
    #[must_use]
    pub fn resolve(&self, checks: &[String], files: &[PathBuf]) -> ResolvedRules {
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
    pub fn resolve_by_category(
        &self,
        checks: &[String],
        files: &[PathBuf],
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
        files: &[PathBuf],
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
    fn rule_matches_files(rule: &Rule, files: &[PathBuf]) -> bool {
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
    fn matches_any_file(pattern: &str, files: &[PathBuf]) -> bool {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::rules::{Rule, Severity};

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
    fn test_resolve_by_category_all() {
        let config = RulesConfig::defaults();
        let category_rules = config.resolve_by_category(&[], &[]);

        // Should have all categories from defaults
        assert!(category_rules.category_count() >= 4);
        assert!(category_rules.get("security").is_some());
        assert!(category_rules.get("style").is_some());
    }

    #[test]
    fn test_resolve_by_category_specific() {
        let config = RulesConfig::defaults();
        let checks = vec!["security".to_string(), "style".to_string()];
        let category_rules = config.resolve_by_category(&checks, &[]);

        // Should only have the two requested categories
        assert_eq!(category_rules.category_count(), 2);
        assert!(category_rules.get("security").is_some());
        assert!(category_rules.get("style").is_some());
        assert!(category_rules.get("performance").is_none());
    }

    #[test]
    fn test_resolve_by_category_case_insensitive() {
        let config = RulesConfig::defaults();
        let checks = vec!["SECURITY".to_string()];
        let category_rules = config.resolve_by_category(&checks, &[]);

        assert_eq!(category_rules.category_count(), 1);
        assert!(category_rules.get("security").is_some());
    }

    #[test]
    fn test_resolve_by_category_with_files() {
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

        // With .py file, only all-files rule applies
        let files = vec![PathBuf::from("script.py")];
        let category_rules = config.resolve_by_category(&[], &files);
        let test_rules = category_rules.get("test").unwrap();
        assert_eq!(test_rules.len(), 1);
        assert_eq!(test_rules.rules[0].name, "all-files");
    }

    #[test]
    fn test_resolve_by_category_flatten_matches_resolve() {
        let config = RulesConfig::defaults();
        let checks = vec!["security".to_string()];
        let files: Vec<PathBuf> = Vec::new();

        let resolved = config.resolve(&checks, &files);
        let category_resolved = config.resolve_by_category(&checks, &files);
        let flattened = category_resolved.flatten();

        // Both should have the same rules
        assert_eq!(resolved.len(), flattened.len());
        for (r, f) in resolved.rules.iter().zip(flattened.rules.iter()) {
            assert_eq!(r.name, f.name);
            assert_eq!(r.category, f.category);
        }
    }
}
