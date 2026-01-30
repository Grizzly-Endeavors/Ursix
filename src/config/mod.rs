//! Configuration management for Ursix
//!
//! Configuration is loaded from multiple sources with the following priority:
//! 1. CLI flags (highest priority)
//! 2. Environment variables (URSIX_*)
//! 3. Project config (.ursix/config.toml)
//! 4. Global config (~/.config/ursix/config.toml)
//! 5. Defaults (lowest priority)

mod types;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub use types::{PromptsConfig, Provider, TokenizerMode};

use crate::llm::RetryConfig;

/// Default model to use
pub const DEFAULT_MODEL: &str = "llama3.2";

/// Default Ollama API URL
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Default URL for OpenAI-compatible API
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";

/// Default URL for Gemini API
pub const DEFAULT_GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Default timeout for LLM requests in seconds
///
/// This timeout applies to individual LLM API calls. For chunked operations,
/// each chunk has its own timeout. A 60-second timeout provides reasonable
/// protection against hung connections while allowing time for complex queries.
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// LLM provider to use
    pub provider: Provider,

    /// Model identifier
    pub model: String,

    /// Provider API base URL (overrides default for selected provider)
    pub provider_url: Option<String>,

    /// API key for authenticated providers
    pub api_key: Option<String>,

    /// Working directory for file operations
    pub working_dir: PathBuf,

    /// Tokenizer mode for token counting
    pub tokenizer_mode: TokenizerMode,

    /// Retry configuration for transient failures
    #[allow(clippy::struct_field_names)]
    pub retry_config: RetryConfig,

    /// Timeout for LLM requests in seconds (default: 60)
    pub timeout_secs: u64,

    /// Custom prompts configuration
    pub prompts: PromptsConfig,
}

impl Config {
    /// Load configuration from all sources (files, env vars)
    ///
    /// Does not apply CLI overrides - those should be applied after calling this.
    ///
    /// # Errors
    /// Returns error if config files exist but cannot be parsed
    pub fn load() -> Result<Self> {
        // Load .env file if present (silent failure OK - file may not exist)
        let _ = dotenvy::dotenv();

        let mut config = Self::default();

        // Load global config
        if let Some(global_path) = Self::global_config_path()
            && global_path.exists()
        {
            let file_config = ConfigFile::load(&global_path)?;
            config.merge_file(&file_config);
        }

        // Load project config (walks up directory tree)
        if let Some(project_path) = Self::find_project_config() {
            let file_config = ConfigFile::load(&project_path)?;
            config.merge_file(&file_config);
        }

        // Apply environment variables
        config.apply_env();

        Ok(config)
    }

    /// Get the global config file path
    fn global_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("ursix/config.toml"))
    }

    /// Find project config by walking up the directory tree
    fn find_project_config() -> Option<PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let config_path = dir.join(".ursix").join("config.toml");
            if config_path.exists() {
                return Some(config_path);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Merge values from a config file
    fn merge_file(&mut self, file: &ConfigFile) {
        if let Some(provider) = file.provider {
            self.provider = provider;
        }
        if let Some(ref model) = file.model {
            self.model.clone_from(model);
        }
        if let Some(ref url) = file.provider_url {
            self.provider_url = Some(url.clone());
        }
        if let Some(mode) = file.tokenizer_mode {
            self.tokenizer_mode = mode;
        }
        if let Some(timeout) = file.timeout_secs {
            self.timeout_secs = timeout;
        }
        if let Some(ref prompts) = file.prompts {
            self.prompts.merge(prompts);
        }
    }

    /// Apply environment variable overrides
    fn apply_env(&mut self) {
        if let Ok(provider) = std::env::var("URSIX_PROVIDER")
            && let Ok(p) = provider.parse()
        {
            self.provider = p;
        }
        if let Ok(model) = std::env::var("URSIX_MODEL") {
            self.model = model;
        }
        if let Ok(url) = std::env::var("URSIX_PROVIDER_URL") {
            self.provider_url = Some(url);
        }
        // Check URSIX_API_KEY, then provider-specific keys as fallback
        if let Ok(key) = std::env::var("URSIX_API_KEY") {
            self.api_key = Some(key);
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            self.api_key = Some(key);
        } else if let Ok(key) = std::env::var("GOOGLE_API_KEY") {
            self.api_key = Some(key);
        }
        if let Ok(mode) = std::env::var("URSIX_TOKENIZER_MODE")
            && let Ok(m) = mode.parse()
        {
            self.tokenizer_mode = m;
        }
        if let Ok(timeout) = std::env::var("URSIX_TIMEOUT")
            && let Ok(t) = timeout.parse()
        {
            self.timeout_secs = t;
        }
    }

    /// Get the effective provider URL based on provider type and overrides
    #[must_use]
    pub fn effective_provider_url(&self) -> &str {
        self.provider_url.as_deref().unwrap_or(match self.provider {
            Provider::Ollama => DEFAULT_OLLAMA_URL,
            Provider::OpenAi => DEFAULT_OPENAI_URL,
            Provider::Gemini => DEFAULT_GEMINI_URL,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: Provider::default(),
            model: String::from(DEFAULT_MODEL),
            provider_url: None,
            api_key: None,
            working_dir: std::env::current_dir().unwrap_or_else(|e| {
                tracing::warn!(error = %e, "failed to get current directory, using '.'");
                PathBuf::from(".")
            }),
            tokenizer_mode: TokenizerMode::default(),
            retry_config: RetryConfig::default(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            prompts: PromptsConfig::default(),
        }
    }
}

/// Configuration file format (TOML)
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    /// LLM provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,

    /// Model to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Provider API base URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_url: Option<String>,

    /// Tokenizer mode (heuristic or full)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokenizer_mode: Option<TokenizerMode>,

    /// Timeout for LLM requests in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,

    /// Custom prompts configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsConfig>,
    // Note: API keys are NOT stored in config files for security
}

impl ConfigFile {
    /// Load config from a TOML file
    ///
    /// # Errors
    /// Returns error if file cannot be read or parsed
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).context(format!("failed to read {}", path.display()))?;
        toml::from_str(&content).context(format!("failed to parse {}", path.display()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.provider, Provider::Ollama);
        assert_eq!(config.model, DEFAULT_MODEL);
        assert!(config.provider_url.is_none());
        assert!(config.api_key.is_none());
        assert_eq!(config.timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn test_effective_provider_url_default_ollama() {
        let config = Config::default();
        assert_eq!(config.effective_provider_url(), DEFAULT_OLLAMA_URL);
    }

    #[test]
    fn test_effective_provider_url_default_openai() {
        let config = Config {
            provider: Provider::OpenAi,
            ..Config::default()
        };
        assert_eq!(config.effective_provider_url(), DEFAULT_OPENAI_URL);
    }

    #[test]
    fn test_effective_provider_url_default_gemini() {
        let config = Config {
            provider: Provider::Gemini,
            ..Config::default()
        };
        assert_eq!(config.effective_provider_url(), DEFAULT_GEMINI_URL);
    }

    #[test]
    fn test_effective_provider_url_with_override() {
        let config = Config {
            provider_url: Some("http://custom:8000/v1".to_string()),
            ..Config::default()
        };
        assert_eq!(config.effective_provider_url(), "http://custom:8000/v1");
    }

    #[test]
    fn test_merge_file() {
        let mut config = Config::default();

        let file = ConfigFile {
            provider: Some(Provider::OpenAi),
            model: Some("custom-model".to_string()),
            provider_url: Some("http://custom:8000/v1".to_string()),
            tokenizer_mode: None,
            timeout_secs: None,
            prompts: None,
        };

        config.merge_file(&file);

        assert_eq!(config.provider, Provider::OpenAi);
        assert_eq!(config.model, "custom-model");
        assert_eq!(
            config.provider_url,
            Some("http://custom:8000/v1".to_string())
        );
    }

    #[test]
    fn test_merge_file_with_timeout() {
        let mut config = Config::default();
        assert_eq!(config.timeout_secs, DEFAULT_TIMEOUT_SECS);

        let file = ConfigFile {
            provider: None,
            model: None,
            provider_url: None,
            tokenizer_mode: None,
            timeout_secs: Some(120),
            prompts: None,
        };

        config.merge_file(&file);
        assert_eq!(config.timeout_secs, 120);
    }

    // Note: Environment variable override tests removed because std::env::set_var
    // is unsafe in Rust 2024 edition and this project forbids unsafe code.
    // The functionality is tested via integration tests instead.

    #[test]
    fn test_partial_config_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("partial.toml");

        // Only model is set
        std::fs::write(&path, "model = \"partial-model\"\n").unwrap();

        let loaded = ConfigFile::load(&path).unwrap();
        assert!(loaded.provider.is_none());
        assert_eq!(loaded.model, Some("partial-model".to_string()));
        assert!(loaded.provider_url.is_none());
    }

    #[test]
    fn test_config_file_with_provider() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("provider.toml");

        std::fs::write(&path, "provider = \"openai\"\nmodel = \"gpt-4\"\n").unwrap();

        let loaded = ConfigFile::load(&path).unwrap();
        assert_eq!(loaded.provider, Some(Provider::OpenAi));
        assert_eq!(loaded.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_config_default_includes_tokenizer_mode() {
        let config = Config::default();
        assert_eq!(config.tokenizer_mode, TokenizerMode::Heuristic);
    }

    #[test]
    fn test_merge_file_with_tokenizer_mode() {
        let mut config = Config::default();
        assert_eq!(config.tokenizer_mode, TokenizerMode::Heuristic);

        let file = ConfigFile {
            provider: None,
            model: None,
            provider_url: None,
            tokenizer_mode: Some(TokenizerMode::Full),
            timeout_secs: None,
            prompts: None,
        };

        config.merge_file(&file);
        assert_eq!(config.tokenizer_mode, TokenizerMode::Full);
    }

    #[test]
    fn test_config_file_with_tokenizer_mode() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("tokenizer.toml");

        std::fs::write(&path, "tokenizer_mode = \"full\"\n").unwrap();

        let loaded = ConfigFile::load(&path).unwrap();
        assert_eq!(loaded.tokenizer_mode, Some(TokenizerMode::Full));
    }

    #[test]
    fn test_config_file_with_timeout() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("timeout.toml");

        std::fs::write(&path, "timeout_secs = 120\n").unwrap();

        let loaded = ConfigFile::load(&path).unwrap();
        assert_eq!(loaded.timeout_secs, Some(120));
    }

    #[test]
    fn test_config_file_with_provider_url() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("provider_url.toml");

        std::fs::write(&path, "provider_url = \"http://localhost:8080/v1\"\n").unwrap();

        let loaded = ConfigFile::load(&path).unwrap();
        assert_eq!(
            loaded.provider_url,
            Some("http://localhost:8080/v1".to_string())
        );
    }

    #[test]
    fn test_find_project_config_finds_config() {
        let temp = TempDir::new().unwrap();
        let ursix_dir = temp.path().join(".ursix");
        std::fs::create_dir(&ursix_dir).unwrap();
        let config_path = ursix_dir.join("config.toml");
        std::fs::write(&config_path, "model = \"test\"\n").unwrap();

        // Change to temp directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let found = Config::find_project_config();
        assert!(found.is_some());
        assert_eq!(found.unwrap(), config_path);

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_find_project_config_walks_up_tree() {
        let temp = TempDir::new().unwrap();
        let ursix_dir = temp.path().join(".ursix");
        std::fs::create_dir(&ursix_dir).unwrap();
        let config_path = ursix_dir.join("config.toml");
        std::fs::write(&config_path, "model = \"test\"\n").unwrap();

        // Create nested directory
        let nested_dir = temp.path().join("nested").join("deeper");
        std::fs::create_dir_all(&nested_dir).unwrap();

        // Change to nested directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested_dir).unwrap();

        // Should find config in parent directory
        let found = Config::find_project_config();
        assert!(found.is_some());
        assert_eq!(found.unwrap(), config_path);

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_find_project_config_returns_none_when_not_found() {
        let temp = TempDir::new().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let found = Config::find_project_config();
        assert!(found.is_none());

        std::env::set_current_dir(original_dir).unwrap();
    }
}
