use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    ChatOptions, LlmClient, LlmError, LlmResponse, Message, Role, ToolCall, ToolDefinition,
};

/// Check if a URL is using insecure HTTP for a remote (non-localhost) server
fn is_insecure_remote_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.starts_with("http://")
        && !url_lower.contains("localhost")
        && !url_lower.contains("127.0.0.1")
        && !url_lower.contains("[::1]")
}

/// Ollama API client
#[derive(Clone)]
pub struct OllamaClient {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let base_url = base_url.into();

        if is_insecure_remote_url(&base_url) {
            tracing::warn!(
                url = %base_url,
                "using unencrypted HTTP for non-localhost API; consider using HTTPS"
            );
        }

        Self {
            client: Client::new(),
            base_url,
            model: model.into(),
        }
    }

    /// List available models from Ollama
    ///
    /// # Errors
    /// Returns error if the API request fails or response cannot be parsed
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_body = response
                .json::<OllamaErrorResponse>()
                .await
                .map_or_else(|_| "unknown error".to_string(), |e| e.error);
            return Err(LlmError::Api(error_body));
        }

        let tags_response: OllamaTagsResponse = response.json().await?;
        Ok(tags_response.models.into_iter().map(|m| m.name).collect())
    }
}

#[async_trait]
impl LlmClient for OllamaClient {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        options: &ChatOptions,
    ) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/api/chat", self.base_url);

        let ollama_messages: Vec<OllamaMessage> = messages.iter().map(Into::into).collect();

        let ollama_tools: Vec<OllamaTool> = tools
            .iter()
            .map(|t| OllamaTool {
                r#type: "function".to_string(),
                function: OllamaFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect();

        let request = OllamaChatRequest {
            model: &self.model,
            messages: ollama_messages,
            tools: if ollama_tools.is_empty() {
                None
            } else {
                Some(ollama_tools)
            },
            stream: false,
            format: if options.json_mode {
                Some("json")
            } else {
                None
            },
        };

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_body = response
                .json::<OllamaErrorResponse>()
                .await
                .map_or_else(|_| "unknown error".to_string(), |e| e.error);
            return Err(LlmError::Api(error_body));
        }

        let chat_response: OllamaChatResponse = response.json().await?;

        let content = chat_response.message.content.unwrap_or_default();
        let tool_calls = chat_response
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, tc)| ToolCall {
                id: format!("call_{i}"),
                name: tc.function.name,
                arguments: tc.function.arguments,
            })
            .collect();

        Ok(LlmResponse::new(content, tool_calls))
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

// Ollama API request/response types

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    stream: bool,
    /// When set to "json", enforces valid JSON output from the model
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a str>,
}

#[derive(Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

impl From<&Message> for OllamaMessage {
    fn from(msg: &Message) -> Self {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        Self {
            role: role.to_string(),
            content: if msg.content.is_empty() {
                None
            } else {
                Some(msg.content.clone())
            },
            tool_calls: msg.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|tc| OllamaToolCall {
                        function: OllamaFunctionCall {
                            name: tc.name.clone(),
                            arguments: tc.arguments.clone(),
                        },
                    })
                    .collect()
            }),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct OllamaTool {
    r#type: String,
    function: OllamaFunction,
}

#[derive(Serialize, Deserialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Serialize, Deserialize)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Deserialize)]
struct OllamaErrorResponse {
    error: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::llm::ChatOptions;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_message_conversion() {
        let msg = Message {
            role: Role::User,
            content: "Hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };

        let ollama_msg: OllamaMessage = (&msg).into();
        assert_eq!(ollama_msg.role, "user");
        assert_eq!(ollama_msg.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_insecure_url_detection() {
        assert!(is_insecure_remote_url("http://api.example.com:11434"));
        assert!(is_insecure_remote_url("http://192.168.1.1:11434"));
        assert!(!is_insecure_remote_url("http://localhost:11434"));
        assert!(!is_insecure_remote_url("http://127.0.0.1:11434"));
        assert!(!is_insecure_remote_url("https://api.example.com"));
    }

    #[tokio::test]
    async fn test_chat_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you today?"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::new(mock_server.uri(), "test-model");
        let messages = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let response = client
            .chat(&messages, &[], &ChatOptions::default())
            .await
            .unwrap();
        assert_eq!(response.content, "Hello! How can I help you today?");
        assert!(response.tool_calls.is_empty());
        assert!(response.is_complete);
    }

    #[tokio::test]
    async fn test_chat_api_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": "model 'nonexistent' not found"
            })))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::new(mock_server.uri(), "nonexistent");
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Api(_)));
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_chat_with_tool_calls() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "function": {
                            "name": "bash",
                            "arguments": {"command": "ls -la"}
                        }
                    }]
                }
            })))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::new(mock_server.uri(), "test-model");
        let messages = vec![Message {
            role: Role::User,
            content: "List files".to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let response = client
            .chat(&messages, &[], &ChatOptions::default())
            .await
            .unwrap();
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "bash");
        assert_eq!(response.tool_calls[0].id, "call_0");
        assert!(!response.is_complete); // Has tool calls, so not complete
    }

    #[tokio::test]
    async fn test_chat_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "internal server error"
            })))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::new(mock_server.uri(), "test-model");
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::Api(_)));
    }

    #[tokio::test]
    async fn test_chat_malformed_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::new(mock_server.uri(), "test-model");
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        // Should fail to parse
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_with_json_mode() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .and(body_partial_json(serde_json::json!({ "format": "json" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "role": "assistant",
                    "content": "{\"key\": \"value\"}"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = OllamaClient::new(mock_server.uri(), "test-model");
        let messages = vec![Message {
            role: Role::User,
            content: "Return JSON".to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let response = client
            .chat(&messages, &[], &ChatOptions::json())
            .await
            .unwrap();
        assert_eq!(response.content, "{\"key\": \"value\"}");
    }
}
