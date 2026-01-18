//! Configuration management for Ursus.rs
//!
//! Configuration is loaded from multiple sources with the following priority:
//! 1. CLI flags (highest priority)
//! 2. Environment variables (URSUS_*)
//! 3. Project config (.ursus.toml in current or parent directories)
//! 4. Global config (~/.config/ursus/config.toml)
//! 5. Defaults (lowest priority)

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default model to use
pub const DEFAULT_MODEL: &str = "llama3.2";

/// Default Ollama API URL
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Default maximum agent turns
pub const DEFAULT_MAX_TURNS: usize = 50;

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Model identifier for Ollama
    pub model: String,

    /// Ollama API base URL
    pub ollama_url: String,

    /// Working directory for file operations
    pub working_dir: PathBuf,

    /// Maximum turns in the agent loop before stopping
    pub max_turns: usize,
}

impl Config {
    /// Create a new config with the specified model and URL
    pub fn new(model: String, ollama_url: String) -> Self {
        Self {
            model,
            ollama_url,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_turns: DEFAULT_MAX_TURNS,
        }
    }

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
        dirs::config_dir().map(|p| p.join("ursus/config.toml"))
    }

    /// Find project config by walking up the directory tree
    fn find_project_config() -> Option<PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let config_path = dir.join(".ursus.toml");
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
        if let Some(ref model) = file.model {
            self.model.clone_from(model);
        }
        if let Some(ref url) = file.ollama_url {
            self.ollama_url.clone_from(url);
        }
        if let Some(turns) = file.max_turns {
            self.max_turns = turns;
        }
    }

    /// Apply environment variable overrides
    fn apply_env(&mut self) {
        if let Ok(model) = std::env::var("URSUS_MODEL") {
            self.model = model;
        }
        if let Ok(url) = std::env::var("URSUS_OLLAMA_URL") {
            self.ollama_url = url;
        }
        if let Ok(turns) = std::env::var("URSUS_MAX_TURNS")
            && let Ok(turns) = turns.parse()
        {
            self.max_turns = turns;
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new(
            String::from(DEFAULT_MODEL),
            String::from(DEFAULT_OLLAMA_URL),
        )
    }
}

/// Configuration file format (TOML)
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Model to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Ollama API URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_url: Option<String>,

    /// Maximum agent turns
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<usize>,
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

    /// Save config to a TOML file
    ///
    /// # Errors
    /// Returns error if file cannot be written
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self).context("failed to serialize config")?;

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context(format!("failed to create {}", parent.display()))?;
        }

        std::fs::write(path, content).context(format!("failed to write {}", path.display()))
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
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.ollama_url, DEFAULT_OLLAMA_URL);
        assert_eq!(config.max_turns, DEFAULT_MAX_TURNS);
    }

    #[test]
    fn test_config_file_roundtrip() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.toml");

        let file = ConfigFile {
            model: Some("test-model".to_string()),
            ollama_url: Some("http://test:1234".to_string()),
            max_turns: Some(100),
        };

        file.save(&path).unwrap();
        let loaded = ConfigFile::load(&path).unwrap();

        assert_eq!(loaded.model, Some("test-model".to_string()));
        assert_eq!(loaded.ollama_url, Some("http://test:1234".to_string()));
        assert_eq!(loaded.max_turns, Some(100));
    }

    #[test]
    fn test_merge_file() {
        let mut config = Config::default();

        let file = ConfigFile {
            model: Some("custom-model".to_string()),
            ollama_url: None,
            max_turns: Some(25),
        };

        config.merge_file(&file);

        assert_eq!(config.model, "custom-model");
        assert_eq!(config.ollama_url, DEFAULT_OLLAMA_URL); // Unchanged
        assert_eq!(config.max_turns, 25);
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
        assert_eq!(loaded.model, Some("partial-model".to_string()));
        assert!(loaded.ollama_url.is_none());
        assert!(loaded.max_turns.is_none());
    }
}
