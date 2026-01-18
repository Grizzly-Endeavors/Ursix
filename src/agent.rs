use std::time::Instant;

use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::config::Config;
use crate::llm::{LlmClient, LlmError, Message, Role};
use crate::tools::{ToolResult, executor};

/// System prompt that defines the agent's behavior and available tools
pub(crate) const SYSTEM_PROMPT: &str = r"You are a helpful coding assistant with access to tools for interacting with the local filesystem and running commands.

Available tools:
- bash: Execute shell commands
- read: Read file contents
- write: Create or overwrite files
- edit: Make precise edits to existing files
- glob: Find files matching patterns
- grep: Search file contents with regex

When given a task, think step by step. Use tools to gather information and make changes. Always verify your work.";

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("max turns ({0}) reached without completion")]
    MaxTurnsExceeded(usize),
}

/// Events emitted during agent execution for TUI updates
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Agent loop started
    Started { turn: usize, max_turns: usize },

    /// LLM call initiated (thinking indicator)
    LlmCallStarted,

    /// LLM responded (content may be empty if only tool calls)
    LlmResponse {
        content: String,
        tool_count: usize,
        turn: usize,
    },

    /// Tool execution started
    ToolStarted {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },

    /// Tool execution completed
    ToolCompleted {
        id: String,
        name: String,
        result: ToolResult,
        duration_ms: u64,
    },

    /// Agent completed successfully
    Completed { final_response: String },

    /// Agent encountered an error
    Error { message: String },

    /// Updated history after agent completes (success or failure)
    HistoryUpdated { history: Vec<Message> },
}

/// The core agent loop that orchestrates LLM calls and tool execution
pub struct Agent<L: LlmClient> {
    config: Config,
    client: L,
}

impl<L: LlmClient> Agent<L> {
    pub fn new(config: Config, client: L) -> Self {
        Self { config, client }
    }

    /// Run the agent loop with an initial user message (creates fresh history)
    ///
    /// Uses the default system prompt.
    ///
    /// # Errors
    /// Returns error if LLM communication fails or `max_turns` exceeded without completion
    pub async fn run(&self, initial_message: &str) -> Result<String, AgentError> {
        self.run_with_prompt(SYSTEM_PROMPT, initial_message).await
    }

    /// Run the agent loop with a custom system prompt
    ///
    /// # Errors
    /// Returns error if LLM communication fails or `max_turns` exceeded without completion
    pub async fn run_with_prompt(
        &self,
        system_prompt: &str,
        initial_message: &str,
    ) -> Result<String, AgentError> {
        let mut messages = vec![Message {
            role: Role::System,
            content: system_prompt.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        self.run_with_history(&mut messages, initial_message).await
    }

    /// Run the agent loop using external message history
    ///
    /// Appends user message, runs turns, returns final assistant response.
    /// Message history is updated in-place for multi-turn support.
    ///
    /// # Errors
    /// Returns error if LLM communication fails or `max_turns` exceeded without completion
    pub async fn run_with_history(
        &self,
        messages: &mut Vec<Message>,
        user_message: &str,
    ) -> Result<String, AgentError> {
        self.run_loop(messages, user_message, None).await
    }

    /// Run the agent loop with event reporting for TUI updates
    ///
    /// Similar to `run_with_history` but emits events for each step through
    /// the provided channel. Use `None` for `event_tx` to disable event reporting.
    ///
    /// # Errors
    /// Returns error if LLM communication fails or `max_turns` exceeded without completion
    pub async fn run_with_events(
        &self,
        messages: &mut Vec<Message>,
        user_message: &str,
        event_tx: Option<mpsc::UnboundedSender<AgentEvent>>,
    ) -> Result<String, AgentError> {
        self.run_loop(messages, user_message, event_tx.as_ref())
            .await
    }

    /// Core agent loop implementation
    ///
    /// Handles the main agent loop logic with optional event reporting.
    /// Events are only sent when `event_tx` is `Some`.
    async fn run_loop(
        &self,
        messages: &mut Vec<Message>,
        user_message: &str,
        event_tx: Option<&mpsc::UnboundedSender<AgentEvent>>,
    ) -> Result<String, AgentError> {
        messages.push(Message {
            role: Role::User,
            content: user_message.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        let tools = executor::all_tool_definitions();

        info!(
            model = %self.client.model_name(),
            max_turns = self.config.max_turns,
            history_len = messages.len(),
            "starting agent loop"
        );

        send_event(
            event_tx,
            AgentEvent::Started {
                turn: 0,
                max_turns: self.config.max_turns,
            },
        );

        for turn in 0..self.config.max_turns {
            match self.process_turn(messages, &tools, turn, event_tx).await {
                TurnResult::Complete(response) => return Ok(response),
                TurnResult::Continue => {}
                TurnResult::Error(e) => return Err(e),
            }
        }

        send_error_with_history(
            event_tx,
            format!(
                "max turns ({}) reached without completion",
                self.config.max_turns
            ),
            messages,
        );
        Err(AgentError::MaxTurnsExceeded(self.config.max_turns))
    }

    /// Process a single turn of the agent loop
    async fn process_turn(
        &self,
        messages: &mut Vec<Message>,
        tools: &[crate::llm::ToolDefinition],
        turn: usize,
        event_tx: Option<&mpsc::UnboundedSender<AgentEvent>>,
    ) -> TurnResult {
        info!(turn, "agent turn");
        send_event(event_tx, AgentEvent::LlmCallStarted);

        let response = match self.client.chat(messages, tools).await {
            Ok(r) => r,
            Err(e) => {
                send_error_with_history(event_tx, e.to_string(), messages);
                return TurnResult::Error(e.into());
            }
        };

        debug!(
            content_len = response.content.len(),
            tool_calls = response.tool_calls.len(),
            is_complete = response.is_complete,
            "received LLM response"
        );

        send_event(
            event_tx,
            AgentEvent::LlmResponse {
                content: response.content.clone(),
                tool_count: response.tool_calls.len(),
                turn,
            },
        );

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

        if response.is_complete {
            info!(turn, "agent completed");
            send_event(
                event_tx,
                AgentEvent::Completed {
                    final_response: response.content.clone(),
                },
            );
            send_event(
                event_tx,
                AgentEvent::HistoryUpdated {
                    history: messages.clone(),
                },
            );
            return TurnResult::Complete(response.content);
        }

        for tool_call in &response.tool_calls {
            let result_content = self.execute_tool_with_events(tool_call, event_tx).await;
            messages.push(Message {
                role: Role::Tool,
                content: result_content,
                tool_calls: None,
                tool_call_id: Some(tool_call.id.clone()),
            });
        }

        TurnResult::Continue
    }

    async fn execute_tool_with_events(
        &self,
        tool_call: &crate::llm::ToolCall,
        event_tx: Option<&mpsc::UnboundedSender<AgentEvent>>,
    ) -> String {
        info!(
            tool = %tool_call.name,
            call_id = %tool_call.id,
            "executing tool"
        );

        send_event(
            event_tx,
            AgentEvent::ToolStarted {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
            },
        );

        let start = Instant::now();
        let result = executor::execute_tool(
            &tool_call.name,
            &tool_call.arguments,
            &self.config.working_dir,
        )
        .await;
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        debug!(
            tool = %tool_call.name,
            success = result.success,
            output_len = result.output.len(),
            duration_ms,
            "tool execution complete"
        );

        send_event(
            event_tx,
            AgentEvent::ToolCompleted {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                result: result.clone(),
                duration_ms,
            },
        );

        if result.success {
            result.output
        } else {
            format!(
                "Error: {}",
                result.error.unwrap_or_else(|| "unknown error".to_string())
            )
        }
    }
}

/// Result of processing a single agent turn
enum TurnResult {
    /// Agent completed with final response
    Complete(String),
    /// Continue to next turn
    Continue,
    /// Error occurred
    Error(AgentError),
}

/// Send an event if the channel is available (ignores send errors)
fn send_event(event_tx: Option<&mpsc::UnboundedSender<AgentEvent>>, event: AgentEvent) {
    if let Some(tx) = event_tx {
        let _ = tx.send(event);
    }
}

/// Send error event followed by history update
fn send_error_with_history(
    event_tx: Option<&mpsc::UnboundedSender<AgentEvent>>,
    message: String,
    messages: &[Message],
) {
    send_event(event_tx, AgentEvent::Error { message });
    send_event(
        event_tx,
        AgentEvent::HistoryUpdated {
            history: messages.to_vec(),
        },
    );
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

        let result = agent.run("Hi").await.unwrap();
        assert_eq!(result, "Hello!");
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

        let result = agent.run("Run echo test").await.unwrap();
        assert!(result.contains("test"));
    }

    #[tokio::test]
    async fn test_max_turns_exceeded() {
        // Client that always returns tool calls (never completes)
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

        let result = agent.run("Loop forever").await;
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

        let result = agent.run("Use unknown tool").await.unwrap();
        assert!(result.contains("error"));
    }

    #[tokio::test]
    async fn test_run_with_history_multi_turn() {
        // Test that message history accumulates correctly across multiple calls
        let client = MockClient::new(vec![
            LlmResponse::new("First response".to_string(), vec![]),
            LlmResponse::new("Second response".to_string(), vec![]),
        ]);
        let config = Config::default();
        let agent = Agent::new(config, client);

        // Start with just system prompt
        let mut messages = vec![Message {
            role: Role::System,
            content: SYSTEM_PROMPT.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        // First turn
        let r1 = agent
            .run_with_history(&mut messages, "First question")
            .await
            .unwrap();
        assert_eq!(r1, "First response");
        // Should have: system + user + assistant
        assert_eq!(messages.len(), 3);
        assert!(matches!(messages[1].role, Role::User));
        assert!(matches!(messages[2].role, Role::Assistant));

        // Second turn
        let r2 = agent
            .run_with_history(&mut messages, "Second question")
            .await
            .unwrap();
        assert_eq!(r2, "Second response");
        // Should have: system + user + assistant + user + assistant
        assert_eq!(messages.len(), 5);
        assert!(matches!(messages[3].role, Role::User));
        assert!(matches!(messages[4].role, Role::Assistant));
    }
}
