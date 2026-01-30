//! Tests for default rules.

#[cfg(test)]
mod tests {
    use crate::rules::RulesConfig;

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
