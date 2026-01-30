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
pub(crate) enum TokenizerMode {
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
pub(crate) enum Provider {
    #[default]
    Ollama,
    OpenAi,
    Gemini,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAi => write!(f, "openai"),
            Self::Gemini => write!(f, "gemini"),
        }
    }
}

impl FromStr for Provider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai" => Ok(Self::OpenAi),
            "gemini" => Ok(Self::Gemini),
            _ => Err(format!(
                "unknown provider: {s} (expected 'ollama', 'openai', or 'gemini')"
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

/// Custom prompts configuration
///
/// Allows overriding default system prompts for each command type.
/// If a prompt is not set, the hardcoded default is used.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct PromptsConfig {
    /// Custom prompt for the review command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,

    /// Custom prompt for commit-msg derive type
    #[serde(rename = "commit-msg", skip_serializing_if = "Option::is_none")]
    pub commit_msg: Option<String>,

    /// Custom prompt for explanation derive type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,

    /// Custom prompt for summary derive type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Custom prompt for fix command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl PromptsConfig {
    /// Merge another prompts config into this one.
    /// Only non-None values from `other` override values in `self`.
    pub(crate) fn merge(&mut self, other: &Self) {
        if other.review.is_some() {
            self.review.clone_from(&other.review);
        }
        if other.commit_msg.is_some() {
            self.commit_msg.clone_from(&other.commit_msg);
        }
        if other.explanation.is_some() {
            self.explanation.clone_from(&other.explanation);
        }
        if other.summary.is_some() {
            self.summary.clone_from(&other.summary);
        }
        if other.fix.is_some() {
            self.fix.clone_from(&other.fix);
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;

    #[test]
    fn test_provider_parse() {
        assert_eq!(Provider::from_str("ollama").unwrap(), Provider::Ollama);
        assert_eq!(Provider::from_str("openai").unwrap(), Provider::OpenAi);
        assert_eq!(Provider::from_str("gemini").unwrap(), Provider::Gemini);
        assert_eq!(Provider::from_str("OLLAMA").unwrap(), Provider::Ollama);
        assert_eq!(Provider::from_str("OpenAI").unwrap(), Provider::OpenAi);
        assert_eq!(Provider::from_str("GEMINI").unwrap(), Provider::Gemini);
        assert!(Provider::from_str("unknown").is_err());
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(Provider::Ollama.to_string(), "ollama");
        assert_eq!(Provider::OpenAi.to_string(), "openai");
        assert_eq!(Provider::Gemini.to_string(), "gemini");
    }

    #[test]
    fn test_provider_serialize_deserialize() {
        let provider_openai = Provider::OpenAi;
        let json_openai = serde_json::to_string(&provider_openai).unwrap();
        assert_eq!(json_openai, "\"openai\"");

        let deserialized_openai: Provider = serde_json::from_str(&json_openai).unwrap();
        assert_eq!(deserialized_openai, Provider::OpenAi);

        let provider_gemini = Provider::Gemini;
        let json_gemini = serde_json::to_string(&provider_gemini).unwrap();
        assert_eq!(json_gemini, "\"gemini\"");

        let deserialized_gemini: Provider = serde_json::from_str(&json_gemini).unwrap();
        assert_eq!(deserialized_gemini, Provider::Gemini);
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
    fn test_prompts_config_default() {
        let config = PromptsConfig::default();
        assert!(config.review.is_none());
        assert!(config.commit_msg.is_none());
        assert!(config.explanation.is_none());
        assert!(config.summary.is_none());
        assert!(config.fix.is_none());
    }

    #[test]
    fn test_prompts_config_merge() {
        let mut config = PromptsConfig {
            review: Some("original review".to_string()),
            commit_msg: None,
            explanation: None,
            summary: None,
            fix: None,
        };

        let other = PromptsConfig {
            review: None,
            commit_msg: Some("new commit".to_string()),
            explanation: Some("new explain".to_string()),
            summary: None,
            fix: None,
        };

        config.merge(&other);

        // Original review should be preserved (other.review is None)
        assert_eq!(config.review, Some("original review".to_string()));
        // New values should be set
        assert_eq!(config.commit_msg, Some("new commit".to_string()));
        assert_eq!(config.explanation, Some("new explain".to_string()));
        // Still None
        assert!(config.summary.is_none());
        assert!(config.fix.is_none());
    }

    #[test]
    fn test_prompts_config_merge_overwrite() {
        let mut config = PromptsConfig {
            review: Some("original review".to_string()),
            ..Default::default()
        };

        let other = PromptsConfig {
            review: Some("new review".to_string()),
            ..Default::default()
        };

        config.merge(&other);

        // Review should be overwritten
        assert_eq!(config.review, Some("new review".to_string()));
    }

    #[test]
    fn test_prompts_config_serialize_deserialize() {
        let config = PromptsConfig {
            review: Some("custom review".to_string()),
            commit_msg: Some("custom commit".to_string()),
            explanation: None,
            summary: None,
            fix: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PromptsConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.review, Some("custom review".to_string()));
        assert_eq!(deserialized.commit_msg, Some("custom commit".to_string()));
        assert!(deserialized.explanation.is_none());
    }
}
