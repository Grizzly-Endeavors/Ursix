//! The `config` command implementation

use anyhow::Result;

use crate::config::Config;
use crate::output::{CommandOutput, ConfigEntry, ConfigResult, ExitCode, OutputMode};

#[allow(clippy::unnecessary_wraps)]
pub fn cmd_config(
    config: &Config,
    key: Option<String>,
    list: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let api_key_display = if config.api_key.is_some() {
        "[set]".to_string()
    } else {
        "[not set]".to_string()
    };

    let provider_url_display = config
        .provider_url
        .clone()
        .unwrap_or_else(|| format!("[default for {}]", config.provider));

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
                    key: "provider_url".to_string(),
                    value: provider_url_display.clone(),
                },
                ConfigEntry {
                    key: "api_key".to_string(),
                    value: api_key_display.clone(),
                },
                ConfigEntry {
                    key: "tokenizer_mode".to_string(),
                    value: config.tokenizer_mode.to_string(),
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
            "provider_url" => provider_url_display,
            "api_key" => api_key_display,
            "tokenizer_mode" => config.tokenizer_mode.to_string(),
            "working_dir" => config.working_dir.display().to_string(),
            _ => format!("unknown config key: {k}"),
        };
        let result = ConfigResult {
            entries: vec![ConfigEntry { key: k, value: val }],
        };
        println!("{}", result.render(output_mode));
    } else {
        // Show usage help
        let help_text = "Usage: usx config <key> or usx config --list\n\
                         To change settings, edit .ursix.toml directly.";
        match output_mode {
            OutputMode::Human => println!("{help_text}"),
            OutputMode::Json => println!(r#"{{"help": "{}"}}"#, help_text.replace('\n', "\\n")),
        }
    }
    Ok(ExitCode::Success)
}
