//! The `init` command implementation
//!
//! Interactive setup wizard for first-time Ursix configuration.
//! Creates `.ursix/config.toml` configuration and `.ursix/rules.yml` with starter rules.

use std::fs;
use std::io::{self, Write};

use anyhow::{Context, Result};
use dialoguer::{Input, Select};

use crate::config::{ConfigFile, DEFAULT_MODEL, DEFAULT_OLLAMA_URL, Provider};
use crate::llm::ollama::OllamaClient;
use crate::output::{CommandOutput, ExitCode, ExitStatus, InitResult, OutputMode};

/// Options for the init command
#[derive(Debug, Clone)]
pub(crate) struct InitOptions {
    /// Skip connectivity test
    pub skip_test: bool,
    /// Force overwrite existing config files
    pub force: bool,
}

/// Execute the init command
///
/// # Errors
/// Returns error if file operations fail or user cancels
pub(crate) async fn cmd_init(options: InitOptions, output_mode: OutputMode) -> Result<ExitCode> {
    let working_dir = std::env::current_dir().context("failed to get current directory")?;
    let ursix_dir = working_dir.join(".ursix");
    let config_path = ursix_dir.join("config.toml");
    let rules_path = ursix_dir.join("rules.yml");

    let mut warnings = Vec::new();
    let mut files_created = Vec::new();

    // Check for existing config
    if config_path.exists() && !options.force {
        eprintln!("Configuration file .ursix/config.toml already exists.");
        eprintln!("Use --force to overwrite existing configuration.");
        let result = InitResult {
            success: false,
            files_created: vec![],
            provider: String::new(),
            model: String::new(),
            connectivity_test: None,
            warnings: vec!["existing configuration not overwritten".to_string()],
        };
        println!("{}", result.render(output_mode));
        return Ok(result.exit_code());
    }

    println!("Welcome to Ursix! Let's set up your configuration.\n");

    // Detect Ollama availability
    let ollama_available = detect_ollama().await;

    // Provider selection
    let provider = select_provider(ollama_available)?;

    // Model selection with sensible defaults
    let default_model = match provider {
        Provider::Ollama => DEFAULT_MODEL.to_string(),
        Provider::OpenAi => "gpt-4o-mini".to_string(),
        Provider::Gemini => "gemini-2.0-flash".to_string(),
    };

    let model: String = Input::new()
        .with_prompt("Model to use")
        .default(default_model.clone())
        .interact_text()
        .context("failed to read model input")?;

    // For cloud providers, guide API key setup
    if provider == Provider::OpenAi {
        guide_api_key_setup(&mut warnings, "OpenAI", "OPENAI_API_KEY", "sk-...");
    } else if provider == Provider::Gemini {
        guide_api_key_setup(&mut warnings, "Gemini", "GOOGLE_API_KEY", "AIza...");
    }

    // Create config file
    let config_file = ConfigFile {
        provider: Some(provider),
        model: Some(model.clone()),
        provider_url: None,
        tokenizer_mode: None,
        timeout_secs: None,
        prompts: None,
    };

    let config_toml = toml::to_string_pretty(&config_file).context("failed to serialize config")?;

    // Create .ursix directory if it doesn't exist
    if !ursix_dir.exists() {
        fs::create_dir(&ursix_dir).context("failed to create .ursix directory")?;
    }

    fs::write(&config_path, &config_toml).context("failed to write config.toml")?;
    files_created.push(".ursix/config.toml".to_string());

    if !rules_path.exists() || options.force {
        fs::write(&rules_path, STARTER_RULES).context("failed to write rules.yml")?;
        files_created.push(".ursix/rules.yml".to_string());
    }

    // Test connectivity (unless skipped)
    let connectivity_test = if options.skip_test {
        None
    } else {
        Some(test_connectivity(provider, &model).await)
    };

    if let Some(false) = connectivity_test {
        warnings.push(format!(
            "connectivity test failed; verify {provider} is running and model '{model}' is available"
        ));
    }

    let result = InitResult {
        success: true,
        files_created,
        provider: provider.to_string(),
        model,
        connectivity_test,
        warnings,
    };

    println!();
    println!("{}", result.render(output_mode));
    Ok(result.exit_code())
}

/// Detect if Ollama is running on localhost
async fn detect_ollama() -> bool {
    let client = OllamaClient::new(DEFAULT_OLLAMA_URL, "test", 5);
    client.list_models().await.is_ok()
}

/// Interactive provider selection
fn select_provider(ollama_available: bool) -> Result<Provider> {
    let items = if ollama_available {
        vec![
            "ollama (local) - Recommended, detected on your system",
            "openai (cloud) - Requires API key",
            "gemini (cloud) - Requires API key",
        ]
    } else {
        vec![
            "ollama (local) - Not detected, but you can configure it",
            "openai (cloud) - Requires API key",
            "gemini (cloud) - Requires API key",
        ]
    };

    let default = usize::from(!ollama_available);

    let selection = Select::new()
        .with_prompt("Select LLM provider")
        .items(&items)
        .default(default)
        .interact()
        .context("failed to read provider selection")?;

    Ok(match selection {
        0 => Provider::Ollama,
        1 => Provider::OpenAi,
        _ => Provider::Gemini,
    })
}

/// Guide user through API key setup for cloud providers
fn guide_api_key_setup(
    warnings: &mut Vec<String>,
    provider_name: &str,
    fallback_env: &str,
    key_prefix: &str,
) {
    println!();
    println!("For {provider_name}, you need to set the URSIX_API_KEY environment variable.");
    println!();
    println!("Options:");
    println!("  1. Create a .env file in this directory with: URSIX_API_KEY={key_prefix}");
    println!("  2. Export in your shell: export URSIX_API_KEY={key_prefix}");
    println!(
        "  3. Pass via CLI: --provider {provider} URL {key_prefix}",
        provider = provider_name.to_lowercase()
    );
    println!();

    // Check if API key is already set
    if std::env::var("URSIX_API_KEY").is_err() && std::env::var(fallback_env).is_err() {
        warnings.push("URSIX_API_KEY not set; set it before running commands".to_string());
    }
}

/// Test connectivity to the configured provider
async fn test_connectivity(provider: Provider, model: &str) -> bool {
    print!("Testing connectivity... ");
    io::stdout().flush().ok();

    let result = match provider {
        Provider::Ollama => {
            let client = OllamaClient::new(DEFAULT_OLLAMA_URL, model, 10);
            client.list_models().await.is_ok()
        }
        Provider::OpenAi => {
            // For OpenAI, we just check if API key is set
            // We don't actually make a test call to avoid consuming quota
            std::env::var("URSIX_API_KEY").is_ok() || std::env::var("OPENAI_API_KEY").is_ok()
        }
        Provider::Gemini => {
            // For Gemini, we just check if API key is set
            // We don't actually make a test call to avoid consuming quota
            std::env::var("URSIX_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok()
        }
    };

    if result {
        println!("OK");
    } else {
        println!("FAILED");
    }

    result
}

/// Starter rules focused on LLM-detectable issues beyond traditional linters
const STARTER_RULES: &str = r#"# Ursix Review Rules
# These rules focus on issues that LLMs can detect beyond traditional linters.

categories:
  clarity:
    - name: vague-naming
      description: "Flag variables/functions with unclear names (data, temp, result, handle) that don't convey purpose"
      severity: warning
    - name: misleading-names
      description: "Flag names that don't match behavior (e.g., getUser that also modifies state)"
      severity: warning
    - name: redundant-comments
      description: "Flag comments that just restate what the code already does - comments should add context, not narrate"
      severity: info

  robustness:
    - name: error-message-quality
      description: "Error messages should include context for debugging, not just 'failed' or 'invalid input'"
      severity: warning
    - name: missing-edge-cases
      description: "Flag obvious unhandled edge cases (empty input, null, boundary conditions)"
      severity: warning

  maintainability:
    - name: magic-values
      description: "Unexplained magic numbers or strings that should be named constants"
      severity: warning
    - name: boolean-parameter-blindness
      description: "Boolean parameters unclear at call site - consider named parameters, enum, or builder"
      severity: info
"#;

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;

    #[test]
    fn test_starter_rules_valid_yaml() {
        // Verify starter rules parse as valid YAML
        let parsed: serde_yaml::Value = serde_yaml::from_str(STARTER_RULES).unwrap();
        assert!(parsed.get("categories").is_some());
    }

    #[test]
    fn test_starter_rules_has_expected_categories() {
        let parsed: serde_yaml::Value = serde_yaml::from_str(STARTER_RULES).unwrap();
        let categories = parsed.get("categories").unwrap();
        assert!(categories.get("clarity").is_some());
        assert!(categories.get("robustness").is_some());
        assert!(categories.get("maintainability").is_some());
    }
}
