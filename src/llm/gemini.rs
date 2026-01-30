//! Client for Google's Gemini API.
//!
//! Supports chat completion with JSON mode and tool calls.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::http::{HttpClientConfig, SharedHttpClient, map_request_error, warn_if_insecure_remote};
use super::{
    ChatOptions, LlmClient, LlmError, LlmResponse, Message, Role, ToolCall, ToolDefinition,
};

/// Gemini API client
#[derive(Clone)]
pub(crate) struct GeminiClient {
    http: SharedHttpClient,
    base_url: String,
    api_key: String,
    model: String,
}

impl GeminiClient {
    /// Create a new Gemini client
    ///
    /// This creates a new HTTP client internally. For connection reuse across
    /// multiple clients, use [`with_http_client`](Self::with_http_client) instead.
    ///
    /// # Arguments
    /// * `base_url` - Gemini API base URL
    /// * `model` - Model identifier (e.g., "gemini-2.0-flash")
    /// * `api_key` - Google API key (always required)
    /// * `timeout_secs` - Request timeout in seconds
    pub(crate) fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
        timeout_secs: u64,
    ) -> Self {
        let base_url = base_url.into();
        warn_if_insecure_remote(&base_url);
        let http = SharedHttpClient::new(&HttpClientConfig::with_timeout(timeout_secs));

        Self {
            http,
            base_url,
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    /// Create a new client with a shared HTTP client
    ///
    /// Use this constructor to share connection pools across multiple LLM clients.
    pub(crate) fn with_http_client(
        http: SharedHttpClient,
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        let base_url = base_url.into();
        warn_if_insecure_remote(&base_url);

        Self {
            http,
            base_url,
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    fn timeout_secs(&self) -> u64 {
        self.http.timeout_secs()
    }

    /// List available models from the Gemini API
    ///
    /// # Errors
    /// Returns error if the API request fails or response cannot be parsed
    pub(crate) async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let url = format!("{}/models?key={}", self.base_url, self.api_key);

        let response = self
            .http
            .client()
            .get(&url)
            .send()
            .await
            .map_err(|e| map_request_error(e, self.timeout_secs()))?;

        if !response.status().is_success() {
            let status = response.status();
            let raw_body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read response body".to_string());
            let error_body = serde_json::from_str::<GeminiErrorResponse>(&raw_body)
                .map_or_else(|_| raw_body, |e| e.error.message);
            return Err(LlmError::Api(format!("{status}: {error_body}")));
        }

        let models_response: ModelsResponse = response.json().await?;
        Ok(models_response
            .models
            .into_iter()
            .map(|m| {
                // Strip "models/" prefix if present
                m.name
                    .strip_prefix("models/")
                    .unwrap_or(&m.name)
                    .to_string()
            })
            .collect())
    }
}

#[async_trait]
impl LlmClient for GeminiClient {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        options: &ChatOptions,
    ) -> Result<LlmResponse, LlmError> {
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        // Separate system messages from other messages
        let system_instruction = messages
            .iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let contents: Vec<GeminiContent> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(Into::into)
            .collect();

        let gemini_tools: Vec<GeminiTool> = if tools.is_empty() {
            vec![]
        } else {
            vec![GeminiTool {
                function_declarations: tools.iter().map(Into::into).collect(),
            }]
        };

        let generation_config = options.json_mode.then(|| GenerationConfig {
            response_mime_type: Some("application/json".to_string()),
        });

        let request = GenerateContentRequest {
            contents,
            system_instruction: if system_instruction.is_empty() {
                None
            } else {
                Some(SystemInstruction {
                    parts: vec![GeminiPart::Text {
                        text: system_instruction,
                    }],
                })
            },
            tools: if gemini_tools.is_empty() {
                None
            } else {
                Some(gemini_tools)
            },
            generation_config,
        };

        let response = self
            .http
            .client()
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| map_request_error(e, self.timeout_secs()))?;

        if !response.status().is_success() {
            let status = response.status();
            let raw_body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read response body".to_string());
            let error_body = serde_json::from_str::<GeminiErrorResponse>(&raw_body)
                .map_or_else(|_| raw_body, |e| e.error.message);
            return Err(LlmError::Api(format!("{status}: {error_body}")));
        }

        let gemini_response: GenerateContentResponse = response.json().await?;

        let candidate = gemini_response
            .candidates
            .into_iter()
            .next()
            .ok_or_else(|| {
                LlmError::Parse("Gemini API response contained no candidates".to_string())
            })?;

        let mut content = String::new();
        let mut tool_calls = Vec::new();

        for part in candidate.content.parts {
            match part {
                GeminiPart::Text { text } => {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(&text);
                }
                GeminiPart::FunctionCall { function_call } => {
                    tool_calls.push(ToolCall {
                        // Gemini doesn't provide tool call IDs, generate one
                        id: format!("call_{}", tool_calls.len()),
                        name: function_call.name,
                        arguments: function_call.args,
                    });
                }
                GeminiPart::FunctionResponse { .. } => {
                    // Function responses in output are unexpected, skip
                }
            }
        }

        Ok(LlmResponse::new(content, tool_calls))
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

// Gemini API request/response types

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize)]
struct SystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

impl From<&Message> for GeminiContent {
    fn from(msg: &Message) -> Self {
        let role = match msg.role {
            Role::Assistant => "model",
            Role::Tool => "function",
            // System messages are filtered out before conversion; map to "user" as fallback
            Role::User | Role::System => "user",
        };

        let mut parts = Vec::new();

        // Add text content if present
        if !msg.content.is_empty() {
            parts.push(GeminiPart::Text {
                text: msg.content.clone(),
            });
        }

        // Add function calls if present (for assistant messages)
        if let Some(ref calls) = msg.tool_calls {
            for call in calls {
                parts.push(GeminiPart::FunctionCall {
                    function_call: FunctionCall {
                        name: call.name.clone(),
                        args: call.arguments.clone(),
                    },
                });
            }
        }

        // Add function response if this is a tool message
        if msg.role == Role::Tool {
            if let Some(ref tool_call_id) = msg.tool_call_id {
                // Extract the function name from the tool_call_id if possible
                // Otherwise use a placeholder
                parts.push(GeminiPart::FunctionResponse {
                    function_response: FunctionResponse {
                        name: tool_call_id.clone(),
                        response: serde_json::json!({ "result": msg.content }),
                    },
                });
            }
        }

        Self {
            role: role.to_string(),
            parts,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    FunctionCall {
        function_call: FunctionCall,
    },
    #[serde(rename_all = "camelCase")]
    FunctionResponse {
        function_response: FunctionResponse,
    },
}

#[derive(Serialize, Deserialize)]
struct FunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct FunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

impl From<&ToolDefinition> for GeminiFunctionDeclaration {
    fn from(tool: &ToolDefinition) -> Self {
        Self {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.parameters.clone(),
        }
    }
}

#[derive(Deserialize)]
struct GenerateContentResponse {
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Candidate {
    content: GeminiContent,
}

#[derive(Deserialize)]
struct GeminiErrorResponse {
    error: GeminiError,
}

#[derive(Deserialize)]
struct GeminiError {
    message: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    models: Vec<ModelInfo>,
}

#[derive(Deserialize)]
struct ModelInfo {
    name: String,
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
#[expect(clippy::panic, reason = "test code uses panic for assertion")]
mod tests {
    use super::*;
    use crate::llm::ChatOptions;
    use wiremock::matchers::{body_partial_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_message_conversion_user() {
        let msg = Message {
            role: Role::User,
            content: "Hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };

        let gemini_content: GeminiContent = (&msg).into();
        assert_eq!(gemini_content.role, "user");
        assert_eq!(gemini_content.parts.len(), 1);
        match gemini_content.parts.get(0) {
            Some(GeminiPart::Text { text }) => assert_eq!(text, "Hello"),
            _ => panic!("expected text part"),
        }
    }

    #[test]
    fn test_message_conversion_assistant() {
        let msg = Message {
            role: Role::Assistant,
            content: "Response".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };

        let gemini_content: GeminiContent = (&msg).into();
        assert_eq!(gemini_content.role, "model");
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

        let gemini_content: GeminiContent = (&msg).into();
        assert_eq!(gemini_content.role, "model");
        assert_eq!(gemini_content.parts.len(), 1);
        match gemini_content.parts.get(0) {
            Some(GeminiPart::FunctionCall { function_call }) => {
                assert_eq!(function_call.name, "bash");
                assert_eq!(function_call.args, serde_json::json!({"command": "ls"}));
            }
            _ => panic!("expected function call part"),
        }
    }

    #[test]
    fn test_message_conversion_tool() {
        let msg = Message {
            role: Role::Tool,
            content: "result output".to_string(),
            tool_calls: None,
            tool_call_id: Some("bash".to_string()),
        };

        let gemini_content: GeminiContent = (&msg).into();
        assert_eq!(gemini_content.role, "function");
        // Should have both text and function response
        assert_eq!(gemini_content.parts.len(), 2);
    }

    #[test]
    fn test_tool_definition_conversion() {
        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "arg1": {"type": "string"}
                }
            }),
        };

        let gemini_func: GeminiFunctionDeclaration = (&tool).into();
        assert_eq!(gemini_func.name, "test_tool");
        assert_eq!(gemini_func.description, "A test tool");
    }

    #[tokio::test]
    async fn test_chat_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .and(query_param("key", "test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "Hello! How can I help you?"}]
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "test-api-key", 60);
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
        assert_eq!(response.content, "Hello! How can I help you?");
        assert!(response.tool_calls.is_empty());
        assert!(response.is_complete);
    }

    #[tokio::test]
    async fn test_chat_with_system_message() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .and(body_partial_json(serde_json::json!({
                "systemInstruction": {
                    "parts": [{"text": "You are a helpful assistant."}]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "Got it!"}]
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "test-api-key", 60);
        let messages = vec![
            Message {
                role: Role::System,
                content: "You are a helpful assistant.".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: "Hi".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let response = client
            .chat(&messages, &[], &ChatOptions::default())
            .await
            .unwrap();
        assert_eq!(response.content, "Got it!");
    }

    #[tokio::test]
    async fn test_chat_with_tool_calls() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{
                            "functionCall": {
                                "name": "bash",
                                "args": {"command": "ls -la"}
                            }
                        }]
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "test-api-key", 60);
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
        assert!(response.content.is_empty());
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(
            response.tool_calls.get(0).map(|t| &t.name),
            Some(&"bash".to_string())
        );
        assert_eq!(
            response.tool_calls.get(0).map(|t| &t.arguments),
            Some(&serde_json::json!({"command": "ls -la"}))
        );
        assert!(!response.is_complete);
    }

    #[tokio::test]
    async fn test_chat_with_json_mode() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .and(body_partial_json(serde_json::json!({
                "generationConfig": {
                    "responseMimeType": "application/json"
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "{\"status\": \"ok\"}"}]
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "test-api-key", 60);
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

    #[tokio::test]
    async fn test_api_error_401() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {
                    "message": "API key not valid"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "bad-key", 60);
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Api(_)));
        assert!(err.to_string().contains("401"));
        assert!(err.to_string().contains("API key not valid"));
    }

    #[tokio::test]
    async fn test_api_error_429() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": {
                    "message": "Rate limit exceeded"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "test-key", 60);
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Api(_)));
        assert!(err.to_string().contains("429"));
    }

    #[tokio::test]
    async fn test_empty_candidates() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": []
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "test-key", 60);
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Parse(_)));
        assert!(err.to_string().contains("no candidates"));
    }

    #[tokio::test]
    async fn test_list_models_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models"))
            .and(query_param("key", "test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    {"name": "models/gemini-2.0-flash"},
                    {"name": "models/gemini-1.5-pro"}
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "test-api-key", 60);
        let models = client.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert!(models.contains(&"gemini-2.0-flash".to_string()));
        assert!(models.contains(&"gemini-1.5-pro".to_string()));
    }

    #[tokio::test]
    async fn test_list_models_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {
                    "message": "API key not valid"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "bad-key", 60);
        let result = client.list_models().await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Api(_)));
        assert!(err.to_string().contains("401"));
    }

    #[tokio::test]
    async fn test_chat_timeout() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(3)))
            .mount(&mock_server)
            .await;

        let client = GeminiClient::new(mock_server.uri(), "gemini-2.0-flash", "test-key", 1);
        let result = client.chat(&[], &[], &ChatOptions::default()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LlmError::Timeout(1)));
    }

    #[tokio::test]
    async fn test_with_http_client() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "Shared client response"}]
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let http = SharedHttpClient::new(&HttpClientConfig::with_timeout(60));
        let client =
            GeminiClient::with_http_client(http, mock_server.uri(), "gemini-2.0-flash", "test-key");

        let response = client
            .chat(&[], &[], &ChatOptions::default())
            .await
            .unwrap();
        assert_eq!(response.content, "Shared client response");
    }
}
