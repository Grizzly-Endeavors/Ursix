//! Tests for rules resolution.

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use std::path::PathBuf;

    use crate::rules::RulesConfig;

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
        use crate::rules::{Rule, Severity};

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
        let files_rs = vec![PathBuf::from("src/main.rs")];
        let resolved_rs = config.resolve(&[], &files_rs);
        assert_eq!(resolved_rs.rules.len(), 2);

        // With .py files, only all-files rule applies
        let files_py = vec![PathBuf::from("script.py")];
        let resolved_py = config.resolve(&[], &files_py);
        assert_eq!(resolved_py.rules.len(), 1);
        assert_eq!(
            resolved_py.rules.get(0).map(|r| &r.name),
            Some(&"all-files".to_string())
        );
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
        use crate::rules::{Rule, Severity};

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
        assert_eq!(
            test_rules.rules.get(0).map(|r| &r.name),
            Some(&"all-files".to_string())
        );
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
