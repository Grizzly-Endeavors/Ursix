//! Client for OpenAI-compatible chat completion APIs.
//!
//! Supports various providers including Azure, vLLM, LM Studio, and other compatible endpoints.

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

/// OpenAI-compatible API client
#[derive(Clone)]
pub struct OpenAiClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiClient {
    /// Create a new client without authentication (for local servers)
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
            api_key: None,
            model: model.into(),
        }
    }

    /// Create a new client with API key authentication
    pub fn with_api_key(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
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
            api_key: Some(api_key.into()),
            model: model.into(),
        }
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        options: &ChatOptions,
    ) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/chat/completions", self.base_url);

        let openai_messages: Vec<OpenAiMessage> = messages.iter().map(Into::into).collect();

        let openai_tools: Vec<OpenAiTool> = tools
            .iter()
            .map(|t| OpenAiTool {
                r#type: "function".to_string(),
                function: OpenAiFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect();

        let response_format = if options.json_mode {
            Some(ResponseFormat {
                r#type: "json_object".to_string(),
            })
        } else {
            None
        };

        let request = ChatCompletionRequest {
            model: &self.model,
            messages: openai_messages,
            tools: if openai_tools.is_empty() {
                None
            } else {
                Some(openai_tools)
            },
            tool_choice: if tools.is_empty() { None } else { Some("auto") },
            response_format,
        };

        let mut req_builder = self.client.post(&url).json(&request);

        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {key}"));
        }

        let response = req_builder.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .json::<OpenAiErrorResponse>()
                .await
                .map_or_else(|_| "unknown error".to_string(), |e| e.error.message);
            return Err(LlmError::Api(format!("{status}: {error_body}")));
        }

        let chat_response: ChatCompletionResponse = response.json().await?;

        let choice = chat_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Parse("response contained no choices".to_string()))?;

        // OpenAI uses null for content when tool_calls are present
        let content = choice.message.content.unwrap_or_default();

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                // OpenAI returns arguments as a JSON string, need to parse it
                let arguments = match serde_json::from_str(&tc.function.arguments) {
                    Ok(args) => args,
                    Err(e) => {
                        tracing::warn!(
                            tool = %tc.function.name,
                            raw_arguments = %tc.function.arguments,
                            error = %e,
                            "failed to parse tool arguments, using empty object"
                        );
                        serde_json::Value::Object(serde_json::Map::new())
                    }
                };
                ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect();

        Ok(LlmResponse::new(content, tool_calls))
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

// OpenAI API request/response types

/// Response format specification for JSON mode
#[derive(Serialize)]
struct ResponseFormat {
    /// The format type - `json_object` for JSON mode
    r#type: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'a str>,
    /// When set, enforces structured output format from the model
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

impl From<&Message> for OpenAiMessage {
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
                    .map(|tc| OpenAiToolCall {
                        id: tc.id.clone(),
                        r#type: "function".to_string(),
                        function: OpenAiFunctionCall {
                            name: tc.name.clone(),
                            // OpenAI expects arguments as a JSON string
                            arguments: tc.arguments.to_string(),
                        },
                    })
                    .collect()
            }),
            tool_call_id: msg.tool_call_id.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct OpenAiTool {
    r#type: String,
    function: OpenAiFunction,
}

#[derive(Serialize, Deserialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    r#type: String,
    function: OpenAiFunctionCall,
}

#[derive(Serialize, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String, // OpenAI returns arguments as JSON string
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
struct ChatCompletionChoice {
    message: OpenAiResponseMessage,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiError,
}

#[derive(Deserialize)]
struct OpenAiError {
    message: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::llm::ChatOptions;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_message_conversion_user() {
        let msg = Message {
            role: Role::User,
            content: "Hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };

        let openai_msg: OpenAiMessage = (&msg).into();
        assert_eq!(openai_msg.role, "user");
        assert_eq!(openai_msg.content, Some("Hello".to_string()));
        assert!(openai_msg.tool_calls.is_none());
    }

    #[test]
    fn test_message_conversion_assistant_with_tool_calls() {
        let msg = Message {
            role: Role::Assistant,
            content: String::new(),
            tool_calls: Some(vec![ToolCall {
                id: "call_123".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command": "ls"}),
            }]),
            tool_call_id: None,
        };

        let openai_msg: OpenAiMessage = (&msg).into();
        assert_eq!(openai_msg.role, "assistant");
        assert!(openai_msg.content.is_none()); // Empty content becomes None
        let tool_calls = openai_msg.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_123");
        assert_eq!(tool_calls[0].function.name, "bash");
        // Arguments should be JSON string
        assert_eq!(tool_calls[0].function.arguments, r#"{"command":"ls"}"#);
    }

    #[test]
    fn test_message_conversion_tool() {
        let msg = Message {
            role: Role::Tool,
            content: "result output".to_string(),
            tool_calls: None,
            tool_call_id: Some("call_123".to_string()),
        };

        let openai_msg: OpenAiMessage = (&msg).into();
        assert_eq!(openai_msg.role, "tool");
        assert_eq!(openai_msg.content, Some("result output".to_string()));
        assert_eq!(openai_msg.tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_insecure_url_detection() {
        assert!(is_insecure_remote_url("http://api.example.com/v1"));
        assert!(is_insecure_remote_url("http://192.168.1.1:8000/v1"));
        assert!(!is_insecure_remote_url("http://localhost:8000/v1"));
        assert!(!is_insecure_remote_url("http://127.0.0.1:8000/v1"));
        assert!(!is_insecure_remote_url("https://api.openai.com/v1"));
    }

    #[tokio::test]
    async fn test_chat_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Hello! How can I help you today?"
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::new(mock_server.uri(), "gpt-4");
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
    async fn test_chat_with_api_key() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer sk-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Authenticated response"
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::with_api_key(mock_server.uri(), "gpt-4", "sk-test-key");
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
        assert_eq!(response.content, "Authenticated response");
    }

    #[tokio::test]
    async fn test_chat_with_tool_calls() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_abc123",
                            "type": "function",
                            "function": {
                                "name": "bash",
                                "arguments": "{\"command\": \"ls -la\"}"
                            }
                        }]
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::new(mock_server.uri(), "gpt-4");
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
        assert!(response.content.is_empty()); // null becomes empty string
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_abc123");
        assert_eq!(response.tool_calls[0].name, "bash");
        assert_eq!(
            response.tool_calls[0].arguments,
            serde_json::json!({"command": "ls -la"})
        );
        assert!(!response.is_complete); // Has tool calls, so not complete
    }

    #[tokio::test]
    async fn test_api_error_401() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {
                    "message": "Invalid API key",
                    "type": "invalid_request_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::new(mock_server.uri(), "gpt-4");
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Api(_)));
        assert!(err.to_string().contains("401"));
        assert!(err.to_string().contains("Invalid API key"));
    }

    #[tokio::test]
    async fn test_api_error_429() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": {
                    "message": "Rate limit exceeded",
                    "type": "rate_limit_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::new(mock_server.uri(), "gpt-4");
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Api(_)));
        assert!(err.to_string().contains("429"));
    }

    #[tokio::test]
    async fn test_api_error_500() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": {
                    "message": "Internal server error",
                    "type": "server_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::new(mock_server.uri(), "gpt-4");
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::Api(_)));
    }

    #[tokio::test]
    async fn test_empty_choices() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": []
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::new(mock_server.uri(), "gpt-4");
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Parse(_)));
        assert!(err.to_string().contains("no choices"));
    }

    #[tokio::test]
    async fn test_malformed_tool_arguments() {
        let mock_server = MockServer::start().await;

        // Return malformed JSON in arguments - should fall back to empty object
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_123",
                            "type": "function",
                            "function": {
                                "name": "test",
                                "arguments": "not valid json"
                            }
                        }]
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::new(mock_server.uri(), "gpt-4");
        let response = client
            .chat(&[], &[], &ChatOptions::default())
            .await
            .unwrap();

        // Should still succeed with empty arguments object
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].arguments, serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_chat_with_json_mode() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_partial_json(
                serde_json::json!({ "response_format": { "type": "json_object" } }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "{\"status\": \"ok\"}"
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = OpenAiClient::new(mock_server.uri(), "gpt-4");
        let messages = vec![Message {
            role: Role::User,
            content: "Return JSON with status".to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let response = client
            .chat(&messages, &[], &ChatOptions::json())
            .await
            .unwrap();
        assert_eq!(response.content, "{\"status\": \"ok\"}");
    }
}
