//! Configuration management for Ursix
//!
//! Configuration is loaded from multiple sources with the following priority:
//! 1. CLI flags (highest priority)
//! 2. Environment variables (URSIX_*)
//! 3. Project config (.ursix.toml in current or parent directories)
//! 4. Global config (~/.config/ursix/config.toml)
//! 5. Defaults (lowest priority)

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default model to use
pub const DEFAULT_MODEL: &str = "llama3.2";

/// Default Ollama API URL
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Default URL for OpenAI-compatible API
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";

/// Tokenizer mode for token counting
///
/// Controls how tokens are counted for input validation and chunking decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenizerMode {
    /// Fast heuristic-based counting (~4 chars per token)
    ///
    /// This is the default and recommended mode. Uses a simple character-based
    /// approximation that works well for most use cases without external dependencies
    /// or network overhead.
    #[default]
    Heuristic,
    /// Full `HuggingFace` tokenizer (GPT-2)
    ///
    /// More accurate but requires downloading tokenizer data on first use (~2MB)
    /// and has higher CPU overhead per call. Use only when precise token counts
    /// are critical.
    Full,
}

impl fmt::Display for TokenizerMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Heuristic => write!(f, "heuristic"),
            Self::Full => write!(f, "full"),
        }
    }
}

impl FromStr for TokenizerMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "heuristic" => Ok(Self::Heuristic),
            "full" => Ok(Self::Full),
            _ => Err(format!(
                "unknown tokenizer mode: {s} (expected 'heuristic' or 'full')"
            )),
        }
    }
}

impl Serialize for TokenizerMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TokenizerMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// LLM provider selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Provider {
    #[default]
    Ollama,
    OpenAi,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAi => write!(f, "openai"),
        }
    }
}

impl FromStr for Provider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai" => Ok(Self::OpenAi),
            _ => Err(format!(
                "unknown provider: {s} (expected 'ollama' or 'openai')"
            )),
        }
    }
}

impl Serialize for Provider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Provider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// LLM provider to use
    pub provider: Provider,

    /// Model identifier
    pub model: String,

    /// Ollama API base URL
    pub ollama_url: String,

    /// OpenAI-compatible API base URL
    pub openai_url: String,

    /// API key for OpenAI-compatible endpoints (loaded from environment only, never in files)
    pub openai_api_key: Option<String>,

    /// Working directory for file operations
    pub working_dir: PathBuf,

    /// Tokenizer mode for token counting
    pub tokenizer_mode: TokenizerMode,
}

impl Config {
    /// Load configuration from all sources (files, env vars)
    ///
    /// Does not apply CLI overrides - those should be applied after calling this.
    ///
    /// # Errors
    /// Returns error if config files exist but cannot be parsed
    pub fn load() -> Result<Self> {
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
            let config_path = dir.join(".ursix.toml");
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
        if let Some(ref url) = file.ollama_url {
            self.ollama_url.clone_from(url);
        }
        if let Some(ref url) = file.openai_url {
            self.openai_url.clone_from(url);
        }
        if let Some(mode) = file.tokenizer_mode {
            self.tokenizer_mode = mode;
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
        if let Ok(url) = std::env::var("URSIX_OLLAMA_URL") {
            self.ollama_url = url;
        }
        if let Ok(url) = std::env::var("URSIX_OPENAI_URL") {
            self.openai_url = url;
        }
        // Check both URSIX_OPENAI_API_KEY and standard OPENAI_API_KEY
        if let Ok(key) = std::env::var("URSIX_OPENAI_API_KEY") {
            self.openai_api_key = Some(key);
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            self.openai_api_key = Some(key);
        }
        if let Ok(mode) = std::env::var("URSIX_TOKENIZER_MODE")
            && let Ok(m) = mode.parse()
        {
            self.tokenizer_mode = m;
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: Provider::default(),
            model: String::from(DEFAULT_MODEL),
            ollama_url: String::from(DEFAULT_OLLAMA_URL),
            openai_url: String::from(DEFAULT_OPENAI_URL),
            openai_api_key: None,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            tokenizer_mode: TokenizerMode::default(),
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

    /// Ollama API URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_url: Option<String>,

    /// OpenAI-compatible API URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_url: Option<String>,

    /// Tokenizer mode (heuristic or full)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokenizer_mode: Option<TokenizerMode>,
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
        assert_eq!(config.ollama_url, DEFAULT_OLLAMA_URL);
        assert_eq!(config.openai_url, DEFAULT_OPENAI_URL);
        assert!(config.openai_api_key.is_none());
    }

    #[test]
    fn test_provider_parse() {
        assert_eq!(Provider::from_str("ollama").unwrap(), Provider::Ollama);
        assert_eq!(Provider::from_str("openai").unwrap(), Provider::OpenAi);
        assert_eq!(Provider::from_str("OLLAMA").unwrap(), Provider::Ollama);
        assert_eq!(Provider::from_str("OpenAI").unwrap(), Provider::OpenAi);
        assert!(Provider::from_str("unknown").is_err());
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(Provider::Ollama.to_string(), "ollama");
        assert_eq!(Provider::OpenAi.to_string(), "openai");
    }

    #[test]
    fn test_provider_serialize_deserialize() {
        let provider = Provider::OpenAi;
        let json = serde_json::to_string(&provider).unwrap();
        assert_eq!(json, "\"openai\"");

        let deserialized: Provider = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Provider::OpenAi);
    }

    #[test]
    fn test_merge_file() {
        let mut config = Config::default();

        let file = ConfigFile {
            provider: Some(Provider::OpenAi),
            model: Some("custom-model".to_string()),
            ollama_url: None,
            openai_url: Some("http://custom:8000/v1".to_string()),
            tokenizer_mode: None,
        };

        config.merge_file(&file);

        assert_eq!(config.provider, Provider::OpenAi);
        assert_eq!(config.model, "custom-model");
        assert_eq!(config.ollama_url, DEFAULT_OLLAMA_URL); // Unchanged
        assert_eq!(config.openai_url, "http://custom:8000/v1");
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
        assert!(loaded.ollama_url.is_none());
        assert!(loaded.openai_url.is_none());
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
    fn test_tokenizer_mode_default() {
        assert_eq!(TokenizerMode::default(), TokenizerMode::Heuristic);
    }

    #[test]
    fn test_tokenizer_mode_parse() {
        assert_eq!(
            TokenizerMode::from_str("heuristic").unwrap(),
            TokenizerMode::Heuristic
        );
        assert_eq!(
            TokenizerMode::from_str("full").unwrap(),
            TokenizerMode::Full
        );
        assert_eq!(
            TokenizerMode::from_str("HEURISTIC").unwrap(),
            TokenizerMode::Heuristic
        );
        assert_eq!(
            TokenizerMode::from_str("Full").unwrap(),
            TokenizerMode::Full
        );
        assert!(TokenizerMode::from_str("unknown").is_err());
    }

    #[test]
    fn test_tokenizer_mode_display() {
        assert_eq!(TokenizerMode::Heuristic.to_string(), "heuristic");
        assert_eq!(TokenizerMode::Full.to_string(), "full");
    }

    #[test]
    fn test_tokenizer_mode_serialize_deserialize() {
        let mode = TokenizerMode::Full;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"full\"");

        let deserialized: TokenizerMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TokenizerMode::Full);
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
            ollama_url: None,
            openai_url: None,
            tokenizer_mode: Some(TokenizerMode::Full),
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
}
