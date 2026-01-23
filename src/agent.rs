//! Agent loop that orchestrates LLM calls and tool execution

use thiserror::Error;
use tokio::signal;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::llm::{ChatOptions, LlmClient, LlmError, Message, Role};
use crate::tools::executor;

/// Errors that can occur during agent execution
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error(
        "agent exceeded maximum of {0} turns without completing. \
             Try increasing --max-turns or breaking the task into smaller pieces"
    )]
    MaxTurnsExceeded(usize),
}

/// Result of an agent run
#[derive(Debug)]
pub struct AgentResult {
    /// The final content from the agent (may be partial if interrupted)
    pub content: String,
    /// Whether the agent was interrupted by a signal
    pub interrupted: bool,
    /// The turn number at which the agent completed or was interrupted
    pub turns_completed: usize,
}

/// The core agent that orchestrates LLM calls and tool execution
pub struct Agent<L: LlmClient> {
    client: L,
    config: Config,
}

impl<L: LlmClient> Agent<L> {
    /// Create a new agent with the given configuration and LLM client
    pub fn new(config: Config, client: L) -> Self {
        Self { client, config }
    }

    /// Run the agent loop with a system prompt and user message
    ///
    /// Returns the agent result which includes the content and whether
    /// the agent was interrupted by a signal.
    ///
    /// # Errors
    /// Returns error if LLM communication fails or max turns exceeded
    pub async fn run(
        &self,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<AgentResult, AgentError> {
        let mut messages = vec![
            Message {
                role: Role::System,
                content: system_prompt.to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: user_message.to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let tools = executor::all_tool_definitions();

        info!(
            model = %self.client.model_name(),
            max_turns = self.config.max_turns,
            "starting agent loop"
        );

        let options = ChatOptions::default();
        let mut last_content = String::new();

        for turn in 0..self.config.max_turns {
            info!(turn, "processing turn");

            // Race between the LLM call and a SIGINT signal
            let response = tokio::select! {
                biased;

                _ = signal::ctrl_c() => {
                    warn!(turn, "agent interrupted by signal");
                    return Ok(AgentResult {
                        content: last_content,
                        interrupted: true,
                        turns_completed: turn,
                    });
                }

                result = self.client.chat(&messages, &tools, &options) => {
                    result?
                }
            };

            debug!(
                content_len = response.content.len(),
                tool_calls = response.tool_calls.len(),
                is_complete = response.is_complete,
                "received LLM response"
            );

            last_content.clone_from(&response.content);

            // Add assistant message to history
            messages.push(Message {
                role: Role::Assistant,
                content: response.content.clone(),
                tool_calls: if response.tool_calls.is_empty() {
                    None
                } else {
                    Some(response.tool_calls.clone())
                },
                tool_call_id: None,
            });

            // Check if we're done
            if response.is_complete {
                info!(turn, "agent completed");
                return Ok(AgentResult {
                    content: response.content,
                    interrupted: false,
                    turns_completed: turn + 1,
                });
            }

            // Execute tool calls - allow in-progress tools to complete naturally
            for tool_call in &response.tool_calls {
                debug!(
                    tool = %tool_call.name,
                    call_id = %tool_call.id,
                    "executing tool"
                );

                let result = executor::execute_tool(
                    &tool_call.name,
                    &tool_call.arguments,
                    &self.config.working_dir,
                )
                .await;

                let result_content = if result.success {
                    result.output
                } else {
                    format!(
                        "Error: {}",
                        result.error.unwrap_or_else(|| "unknown error".to_string())
                    )
                };

                debug!(
                    tool = %tool_call.name,
                    success = result.success,
                    output_len = result_content.len(),
                    "tool execution complete"
                );

                messages.push(Message {
                    role: Role::Tool,
                    content: result_content,
                    tool_calls: None,
                    tool_call_id: Some(tool_call.id.clone()),
                });
            }
        }

        Err(AgentError::MaxTurnsExceeded(self.config.max_turns))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::llm::{LlmResponse, ToolCall, ToolDefinition};
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockClient {
        responses: Vec<LlmResponse>,
        call_count: Arc<AtomicUsize>,
    }

    impl MockClient {
        fn new(responses: Vec<LlmResponse>) -> Self {
            Self {
                responses,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait]
    impl LlmClient for MockClient {
        async fn chat(
            &self,
            _messages: &[Message],
            _tools: &[ToolDefinition],
            _options: &ChatOptions,
        ) -> Result<LlmResponse, LlmError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .responses
                .get(idx)
                .cloned()
                .unwrap_or_else(|| LlmResponse::new("done".to_string(), vec![])))
        }

        #[allow(clippy::unnecessary_literal_bound)]
        fn model_name(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_simple_completion() {
        let client = MockClient::new(vec![LlmResponse::new("Hello!".to_string(), vec![])]);
        let config = Config::default();
        let agent = Agent::new(config, client);

        let result = agent.run("You are helpful.", "Hi").await.unwrap();
        assert_eq!(result.content, "Hello!");
        assert!(!result.interrupted);
        assert_eq!(result.turns_completed, 1);
    }

    #[tokio::test]
    async fn test_tool_execution() {
        let client = MockClient::new(vec![
            LlmResponse::new(
                String::new(),
                vec![ToolCall {
                    id: "call_0".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({"command": "echo test"}),
                }],
            ),
            LlmResponse::new("The command output was: test".to_string(), vec![]),
        ]);
        let config = Config::default();
        let agent = Agent::new(config, client);

        let result = agent
            .run("You are helpful.", "Run echo test")
            .await
            .unwrap();
        assert!(result.content.contains("test"));
        assert!(!result.interrupted);
    }

    #[tokio::test]
    async fn test_max_turns_exceeded() {
        let client = MockClient::new(vec![
            LlmResponse::new(
                String::new(),
                vec![ToolCall {
                    id: "call_0".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({"command": "echo loop"}),
                }],
            );
            100 // More than max_turns
        ]);

        let config = Config {
            max_turns: 3,
            ..Config::default()
        };
        let agent = Agent::new(config, client);

        let result = agent.run("You are helpful.", "Loop forever").await;
        assert!(matches!(result, Err(AgentError::MaxTurnsExceeded(3))));
    }

    #[tokio::test]
    async fn test_unknown_tool_continues() {
        let client = MockClient::new(vec![
            LlmResponse::new(
                String::new(),
                vec![ToolCall {
                    id: "call_0".to_string(),
                    name: "nonexistent_tool".to_string(),
                    arguments: serde_json::json!({}),
                }],
            ),
            LlmResponse::new("I see there was an error.".to_string(), vec![]),
        ]);
        let config = Config::default();
        let agent = Agent::new(config, client);

        let result = agent
            .run("You are helpful.", "Use unknown tool")
            .await
            .unwrap();
        assert!(result.content.contains("error"));
        assert!(!result.interrupted);
    }

    #[tokio::test]
    async fn test_multiple_tool_calls_in_single_turn() {
        let client = MockClient::new(vec![
            LlmResponse::new(
                String::new(),
                vec![
                    ToolCall {
                        id: "call_0".to_string(),
                        name: "bash".to_string(),
                        arguments: serde_json::json!({"command": "echo first"}),
                    },
                    ToolCall {
                        id: "call_1".to_string(),
                        name: "bash".to_string(),
                        arguments: serde_json::json!({"command": "echo second"}),
                    },
                ],
            ),
            LlmResponse::new("Both commands executed.".to_string(), vec![]),
        ]);
        let config = Config::default();
        let agent = Agent::new(config, client);

        let result = agent
            .run("You are helpful.", "Run two commands")
            .await
            .unwrap();
        assert_eq!(result.content, "Both commands executed.");
        assert!(!result.interrupted);
    }
}
