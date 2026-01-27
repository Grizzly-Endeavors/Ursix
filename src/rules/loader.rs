//! Rules configuration loading and merging.

use std::path::Path;

use anyhow::{Context, Result};

use super::RulesConfig;

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
    fn load_from_file(path: &Path) -> Result<Self> {
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::rules::{Rule, Severity};

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
    fn test_load_errors_when_no_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = RulesConfig::load(temp_dir.path());
        // Should error with helpful message
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no rules.yml found"));
        assert!(err.contains("usx init"));
    }
}
