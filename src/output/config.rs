//! Output types for the `config` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Result from the `config` command
#[derive(Debug, Serialize)]
pub struct ConfigResult {
    /// Configuration entries
    pub entries: Vec<ConfigEntry>,
}

/// A single configuration entry
#[derive(Debug, Serialize)]
pub struct ConfigEntry {
    /// Configuration key
    pub key: String,
    /// Configuration value
    pub value: String,
}

impl CommandOutput for ConfigResult {
    fn render_human(&self) -> String {
        self.entries
            .iter()
            .map(|e| format!("{} = {}", e.key, e.value))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl ExitStatus for ConfigResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_config_result_human_output() {
        let result = ConfigResult {
            entries: vec![
                ConfigEntry {
                    key: "model".to_string(),
                    value: "llama3.2".to_string(),
                },
                ConfigEntry {
                    key: "max_turns".to_string(),
                    value: "50".to_string(),
                },
            ],
        };
        let output = result.render_human();
        assert!(output.contains("model = llama3.2"));
        assert!(output.contains("max_turns = 50"));
    }

    #[test]
    fn test_config_result_exit_status() {
        let result = ConfigResult { entries: vec![] };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }
}
