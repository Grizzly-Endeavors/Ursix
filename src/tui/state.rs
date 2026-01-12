use crate::agent::{AgentEvent, SYSTEM_PROMPT};
use crate::config::Config;
use crate::llm::{Message, Role};
use crate::tools::ToolResult;

/// Represents a single tool execution with UI state
#[derive(Debug, Clone)]
pub struct ToolExecution {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: Option<ToolResult>,
    pub duration_ms: Option<u64>,
    pub collapsed: bool,
}

impl ToolExecution {
    pub fn new(id: String, name: String, arguments: serde_json::Value) -> Self {
        Self {
            id,
            name,
            arguments,
            result: None,
            duration_ms: None,
            collapsed: true,
        }
    }
}

/// A message in the conversation view
#[derive(Debug, Clone)]
pub enum DisplayMessage {
    User(String),
    Assistant {
        content: String,
        tool_executions: Vec<ToolExecution>,
    },
    Error(String),
}

/// Focus state for keyboard navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Input,
    Messages,
}

/// Agent execution status
#[derive(Debug, Clone, Default)]
pub enum AgentStatus {
    #[default]
    Idle,
    Thinking,
    ExecutingTool {
        name: String,
    },
    Error(String),
}

/// Main application state
pub struct AppState {
    /// Display messages for the conversation view
    pub messages: Vec<DisplayMessage>,

    /// Current input text
    pub input: String,

    /// Cursor position in input
    pub cursor_position: usize,

    /// Scroll offset for messages
    pub scroll_offset: usize,

    /// Current focus (input or messages)
    pub focus: Focus,

    /// Agent execution status
    pub status: AgentStatus,

    /// Current turn number
    pub current_turn: usize,

    /// Maximum turns
    pub max_turns: usize,

    /// Tick counter for animations
    pub tick: usize,

    /// Runtime configuration
    pub config: Config,

    /// LLM message history (for multi-turn conversations)
    pub history: Vec<Message>,

    /// Index of currently selected message (for navigation)
    pub selected_message: Option<usize>,

    /// Index of currently selected tool within a message
    pub selected_tool: Option<usize>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        // Initialize history with system prompt
        let history = vec![Message {
            role: Role::System,
            content: SYSTEM_PROMPT.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        Self {
            messages: Vec::new(),
            input: String::new(),
            cursor_position: 0,
            scroll_offset: 0,
            focus: Focus::Input,
            status: AgentStatus::Idle,
            current_turn: 0,
            max_turns: config.max_turns,
            tick: 0,
            config,
            history,
            selected_message: None,
            selected_tool: None,
        }
    }

    /// Handle an agent event and update state accordingly
    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Started { max_turns, .. } => {
                self.status = AgentStatus::Thinking;
                self.max_turns = max_turns;
                self.current_turn = 0;
            }
            AgentEvent::LlmCallStarted => {
                self.status = AgentStatus::Thinking;
            }
            AgentEvent::LlmResponse {
                content,
                tool_count,
                turn,
            } => {
                self.current_turn = turn;
                // Create assistant message with empty tool executions
                // Tool executions will be added by subsequent ToolStarted/ToolCompleted events
                if tool_count > 0 || !content.is_empty() {
                    self.messages.push(DisplayMessage::Assistant {
                        content,
                        tool_executions: Vec::new(),
                    });
                }
            }
            AgentEvent::ToolStarted {
                id,
                name,
                arguments,
            } => {
                self.status = AgentStatus::ExecutingTool { name: name.clone() };
                // Add tool execution to the last assistant message
                if let Some(DisplayMessage::Assistant {
                    tool_executions, ..
                }) = self.messages.last_mut()
                {
                    tool_executions.push(ToolExecution::new(id, name, arguments));
                }
            }
            AgentEvent::ToolCompleted {
                id,
                result,
                duration_ms,
                ..
            } => {
                // Update the tool execution in the last assistant message
                if let Some(DisplayMessage::Assistant {
                    tool_executions, ..
                }) = self.messages.last_mut()
                    && let Some(tool) = tool_executions.iter_mut().find(|t| t.id == id)
                {
                    tool.result = Some(result);
                    tool.duration_ms = Some(duration_ms);
                }
            }
            AgentEvent::Completed { .. } => {
                self.status = AgentStatus::Idle;
            }
            AgentEvent::Error { message } => {
                self.status = AgentStatus::Error(message.clone());
                self.messages.push(DisplayMessage::Error(message));
            }
            AgentEvent::HistoryUpdated { history } => {
                self.history = history;
            }
        }
    }

    /// Add a user message to the display
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(DisplayMessage::User(content));
    }

    /// Insert a character at the current cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    /// Delete the character before the cursor
    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position);
        }
    }

    /// Move cursor left
    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    /// Move cursor right
    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            self.cursor_position += 1;
        }
    }

    /// Move cursor to start
    pub fn move_cursor_start(&mut self) {
        self.cursor_position = 0;
    }

    /// Move cursor to end
    pub fn move_cursor_end(&mut self) {
        self.cursor_position = self.input.len();
    }

    /// Clear the input
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor_position = 0;
    }

    /// Take the current input (returns and clears it)
    pub fn take_input(&mut self) -> String {
        let input = std::mem::take(&mut self.input);
        self.cursor_position = 0;
        input
    }

    /// Toggle focus between input and messages
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Input => Focus::Messages,
            Focus::Messages => Focus::Input,
        };
    }

    /// Scroll messages up
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll messages down
    pub fn scroll_down(&mut self, max_offset: usize) {
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
        }
    }

    /// Toggle collapse state of the selected tool
    pub fn toggle_tool_collapse(&mut self) {
        if let (Some(msg_idx), Some(tool_idx)) = (self.selected_message, self.selected_tool)
            && let Some(DisplayMessage::Assistant {
                tool_executions, ..
            }) = self.messages.get_mut(msg_idx)
            && let Some(tool) = tool_executions.get_mut(tool_idx)
        {
            tool.collapsed = !tool.collapsed;
        }
    }

    /// Increment tick for animations
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Check if the agent is currently running
    pub fn is_agent_running(&self) -> bool {
        !matches!(self.status, AgentStatus::Idle | AgentStatus::Error(_))
    }

    /// Get spinner character based on tick
    pub fn spinner_char(&self) -> char {
        const SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        SPINNER_CHARS[self.tick % SPINNER_CHARS.len()]
    }

    /// Clear conversation but keep system prompt
    pub fn clear_conversation(&mut self) {
        self.messages.clear();
        self.history = vec![Message {
            role: Role::System,
            content: SYSTEM_PROMPT.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];
        self.scroll_offset = 0;
        self.selected_message = None;
        self.selected_tool = None;
        self.status = AgentStatus::Idle;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tui::test_helpers::default_config;

    #[test]
    fn test_new_state_initializes_correctly() {
        let state = AppState::new(default_config());
        assert!(state.messages.is_empty());
        assert!(state.input.is_empty());
        assert_eq!(state.focus, Focus::Input);
        assert!(matches!(state.status, AgentStatus::Idle));
        assert_eq!(state.history.len(), 1); // System prompt
    }

    #[test]
    fn test_insert_and_delete_char() {
        let mut state = AppState::new(default_config());
        state.insert_char('h');
        state.insert_char('i');
        assert_eq!(state.input, "hi");
        assert_eq!(state.cursor_position, 2);

        state.delete_char();
        assert_eq!(state.input, "h");
        assert_eq!(state.cursor_position, 1);
    }

    #[test]
    fn test_cursor_movement() {
        let mut state = AppState::new(default_config());
        state.input = "hello".to_string();
        state.cursor_position = 2;

        state.move_cursor_left();
        assert_eq!(state.cursor_position, 1);

        state.move_cursor_right();
        assert_eq!(state.cursor_position, 2);

        state.move_cursor_start();
        assert_eq!(state.cursor_position, 0);

        state.move_cursor_end();
        assert_eq!(state.cursor_position, 5);
    }

    #[test]
    fn test_toggle_focus() {
        let mut state = AppState::new(default_config());
        assert_eq!(state.focus, Focus::Input);

        state.toggle_focus();
        assert_eq!(state.focus, Focus::Messages);

        state.toggle_focus();
        assert_eq!(state.focus, Focus::Input);
    }

    #[test]
    fn test_handle_agent_started() {
        let mut state = AppState::new(default_config());
        state.handle_agent_event(AgentEvent::Started {
            turn: 0,
            max_turns: 50,
        });
        assert!(matches!(state.status, AgentStatus::Thinking));
        assert_eq!(state.max_turns, 50);
    }

    #[test]
    fn test_handle_tool_events() {
        let mut state = AppState::new(default_config());

        // First add an assistant message
        state.handle_agent_event(AgentEvent::LlmResponse {
            content: "Let me run a command".to_string(),
            tool_count: 1,
            turn: 0,
        });

        // Then tool started
        state.handle_agent_event(AgentEvent::ToolStarted {
            id: "call_0".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "echo hello"}),
        });

        assert!(matches!(
            state.status,
            AgentStatus::ExecutingTool { ref name } if name == "bash"
        ));

        // Check tool was added
        let DisplayMessage::Assistant {
            tool_executions, ..
        } = state.messages.last().unwrap()
        else {
            unreachable!("Expected assistant message");
        };
        assert_eq!(tool_executions.len(), 1);
        assert_eq!(tool_executions[0].name, "bash");
        assert!(tool_executions[0].result.is_none());

        // Tool completed
        state.handle_agent_event(AgentEvent::ToolCompleted {
            id: "call_0".to_string(),
            name: "bash".to_string(),
            result: ToolResult::success("hello"),
            duration_ms: 100,
        });

        // Check tool result was updated
        if let Some(DisplayMessage::Assistant {
            tool_executions, ..
        }) = state.messages.last()
        {
            assert!(tool_executions[0].result.is_some());
            assert_eq!(tool_executions[0].duration_ms, Some(100));
        }
    }

    #[test]
    fn test_spinner_cycles() {
        let mut state = AppState::new(default_config());
        let first_char = state.spinner_char();
        state.tick();
        let second_char = state.spinner_char();
        assert_ne!(first_char, second_char);
    }

    #[test]
    fn test_clear_conversation() {
        let mut state = AppState::new(default_config());
        state.add_user_message("Hello".to_string());
        state.messages.push(DisplayMessage::Assistant {
            content: "Hi!".to_string(),
            tool_executions: vec![],
        });
        assert_eq!(state.messages.len(), 2);

        state.clear_conversation();
        assert!(state.messages.is_empty());
        assert_eq!(state.history.len(), 1); // System prompt preserved
    }
}
