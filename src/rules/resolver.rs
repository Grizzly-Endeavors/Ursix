//! Rules resolution based on categories and file patterns.

use std::path::PathBuf;

use super::{CategoryResolvedRules, ResolvedRule, ResolvedRules, RulesConfig};

impl RulesConfig {
    /// Resolve rules based on selected categories and file patterns.
    ///
    /// # Arguments
    /// * `checks` - Category names to include (empty means all categories)
    /// * `files` - File paths being reviewed (for pattern matching)
    #[must_use]
    pub fn resolve(&self, checks: &[String], files: &[PathBuf]) -> ResolvedRules {
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

        for (category, rules) in &self.categories {
            // Skip if checks are specified and this category isn't in the list
            if !checks.is_empty() && !checks.iter().any(|c| c.eq_ignore_ascii_case(category)) {
                continue;
            }

            let mut resolved = Vec::new();
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

            // Sort by name for consistent output within category
            resolved.sort_by(|a, b| a.name.cmp(&b.name));

            if !resolved.is_empty() {
                category_rules.insert(category.clone(), ResolvedRules { rules: resolved });
            } else if checks.iter().any(|c| c.eq_ignore_ascii_case(category)) {
                // User explicitly requested this category but no rules matched the files
                eprintln!(
                    "warning: category '{category}' has no rules matching the specified files"
                );
            }
        }

        category_rules
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
