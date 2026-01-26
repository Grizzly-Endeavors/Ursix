//! Configuration type definitions
//!
//! This module defines enums and types used for configuration options.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
}
