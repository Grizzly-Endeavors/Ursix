use std::path::PathBuf;

/// Application configuration
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
    pub fn new(model: String, ollama_url: String) -> Self {
        Self {
            model,
            ollama_url,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_turns: 50,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new(
            String::from("llama3.2"),
            String::from("http://localhost:11434"),
        )
    }
}
