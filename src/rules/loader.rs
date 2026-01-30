//! Tests for rules loading.

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use crate::rules::RulesConfig;

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
