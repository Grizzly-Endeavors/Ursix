//! The `config` command implementation

use anyhow::Result;

use crate::config::Config;
use crate::output::{AskResult, CommandOutput, ConfigEntry, ConfigResult, ExitCode, OutputMode};

#[allow(clippy::unnecessary_wraps)]
pub fn cmd_config(
    config: &Config,
    key: Option<String>,
    list: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let api_key_display = if config.openai_api_key.is_some() {
        "[set]".to_string()
    } else {
        "[not set]".to_string()
    };

    if list {
        let result = ConfigResult {
            entries: vec![
                ConfigEntry {
                    key: "provider".to_string(),
                    value: config.provider.to_string(),
                },
                ConfigEntry {
                    key: "model".to_string(),
                    value: config.model.clone(),
                },
                ConfigEntry {
                    key: "ollama_url".to_string(),
                    value: config.ollama_url.clone(),
                },
                ConfigEntry {
                    key: "openai_url".to_string(),
                    value: config.openai_url.clone(),
                },
                ConfigEntry {
                    key: "openai_api_key".to_string(),
                    value: api_key_display.clone(),
                },
                ConfigEntry {
                    key: "max_turns".to_string(),
                    value: config.max_turns.to_string(),
                },
                ConfigEntry {
                    key: "working_dir".to_string(),
                    value: config.working_dir.display().to_string(),
                },
            ],
        };
        println!("{}", result.render(output_mode));
    } else if let Some(k) = key {
        let val = match k.as_str() {
            "provider" => config.provider.to_string(),
            "model" => config.model.clone(),
            "ollama_url" => config.ollama_url.clone(),
            "openai_url" => config.openai_url.clone(),
            "openai_api_key" => api_key_display,
            "max_turns" => config.max_turns.to_string(),
            "working_dir" => config.working_dir.display().to_string(),
            _ => format!("unknown config key: {k}"),
        };
        let result = ConfigResult {
            entries: vec![ConfigEntry { key: k, value: val }],
        };
        println!("{}", result.render(output_mode));
    } else {
        // Show usage help through the render system
        let help_text = "Usage: usx config <key> or usx config --list\n\
                         To change settings, edit .ursix.toml directly.";
        let result = AskResult {
            response: help_text.to_string(),
            turns: 0,
        };
        println!("{}", result.render(output_mode));
    }
    Ok(ExitCode::Success)
}
