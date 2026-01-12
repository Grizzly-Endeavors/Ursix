//! Interactive REPL mode for multi-turn conversations

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use thiserror::Error;
use tracing::info;

use crate::agent::{Agent, AgentError, SYSTEM_PROMPT};
use crate::config::Config;
use crate::llm::ollama::OllamaClient;
use crate::llm::{Message, Role};

#[derive(Error, Debug)]
pub enum ReplError {
    #[error("readline error: {0}")]
    Readline(#[from] ReadlineError),

    #[error("agent error: {0}")]
    Agent(#[from] AgentError),
}

/// Parsed user command
enum Command {
    Exit,
    Clear,
    Help,
    Model(String),
    Prompt(String),
    Unknown(String),
    Empty,
}

/// Interactive session managing conversation state
pub struct Session {
    config: Config,
    messages: Vec<Message>,
}

impl Session {
    /// Create a new interactive session
    pub fn new(config: Config) -> Self {
        let messages = vec![Message {
            role: Role::System,
            content: SYSTEM_PROMPT.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        Self { config, messages }
    }

    /// Run the interactive REPL loop
    ///
    /// # Errors
    /// Returns error if readline fails with an I/O error
    pub async fn run(&mut self) -> Result<(), ReplError> {
        let mut editor = DefaultEditor::new()?;

        println!("rust-code interactive mode");
        println!("Type /help for commands, /exit or Ctrl+D to quit\n");

        loop {
            let prompt = format!("[{}] > ", self.config.model);

            match editor.readline(&prompt) {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    match parse_input(line) {
                        Command::Exit => {
                            println!("Goodbye!");
                            break;
                        }
                        Command::Clear => {
                            self.clear_history();
                            println!("Conversation cleared.\n");
                        }
                        Command::Help => print_help(),
                        Command::Model(name) => {
                            self.switch_model(&name);
                            println!("Switched to model: {name}\n");
                        }
                        Command::Prompt(text) => {
                            self.handle_prompt(&text).await;
                        }
                        Command::Unknown(cmd) => {
                            println!("Unknown command: {cmd}");
                            println!("Type /help for available commands.\n");
                        }
                        Command::Empty => {}
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("^C (use /exit or Ctrl+D to quit)");
                }
                Err(ReadlineError::Eof) => {
                    println!("Goodbye!");
                    break;
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(())
    }

    /// Clear conversation history, keeping only the system prompt
    fn clear_history(&mut self) {
        self.messages.truncate(1);
    }

    /// Switch to a different model
    fn switch_model(&mut self, model: &str) {
        self.config.model = model.to_string();
    }

    /// Execute a prompt and print the response
    async fn handle_prompt(&mut self, prompt: &str) {
        match self.execute_prompt(prompt).await {
            Ok(response) => println!("{response}\n"),
            Err(e) => eprintln!("Error: {e}\n"),
        }
    }

    /// Execute a single prompt and get response
    async fn execute_prompt(&mut self, prompt: &str) -> Result<String, AgentError> {
        info!(
            model = %self.config.model,
            history_len = self.messages.len(),
            "executing prompt in interactive mode"
        );

        let client = OllamaClient::new(&self.config.ollama_url, &self.config.model);
        let agent = Agent::new(self.config.clone(), client);

        agent.run_with_history(&mut self.messages, prompt).await
    }
}

fn parse_input(input: &str) -> Command {
    let input = input.trim();

    if input.is_empty() {
        return Command::Empty;
    }

    if !input.starts_with('/') {
        return Command::Prompt(input.to_string());
    }

    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0].to_lowercase();

    match cmd.as_str() {
        "/exit" | "/quit" | "/q" => Command::Exit,
        "/clear" => Command::Clear,
        "/help" | "/h" | "/?" => Command::Help,
        "/model" | "/m" => {
            if parts.len() > 1 && !parts[1].trim().is_empty() {
                Command::Model(parts[1].trim().to_string())
            } else {
                Command::Unknown("/model (missing model name)".to_string())
            }
        }
        _ => Command::Unknown(cmd),
    }
}

fn print_help() {
    println!(
        r"
Available commands:
  /help, /h, /?     Show this help message
  /exit, /quit, /q  Exit interactive mode
  /clear            Clear conversation history
  /model <name>     Switch to a different model

Tips:
  - Type any text to send a prompt to the agent
  - Use Ctrl+D to exit
  - Conversation history is preserved until /clear or exit
"
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exit() {
        assert!(matches!(parse_input("/exit"), Command::Exit));
        assert!(matches!(parse_input("/quit"), Command::Exit));
        assert!(matches!(parse_input("/q"), Command::Exit));
        assert!(matches!(parse_input("  /exit  "), Command::Exit));
    }

    #[test]
    fn test_parse_clear() {
        assert!(matches!(parse_input("/clear"), Command::Clear));
    }

    #[test]
    fn test_parse_help() {
        assert!(matches!(parse_input("/help"), Command::Help));
        assert!(matches!(parse_input("/h"), Command::Help));
        assert!(matches!(parse_input("/?"), Command::Help));
    }

    #[test]
    fn test_parse_model() {
        match parse_input("/model qwen2.5-coder") {
            Command::Model(name) => assert_eq!(name, "qwen2.5-coder"),
            _ => panic!("expected Model command"),
        }

        match parse_input("/m llama3.2") {
            Command::Model(name) => assert_eq!(name, "llama3.2"),
            _ => panic!("expected Model command"),
        }
    }

    #[test]
    fn test_parse_model_missing_arg() {
        assert!(matches!(parse_input("/model"), Command::Unknown(_)));
        assert!(matches!(parse_input("/model   "), Command::Unknown(_)));
    }

    #[test]
    fn test_parse_unknown_command() {
        match parse_input("/foo") {
            Command::Unknown(cmd) => assert_eq!(cmd, "/foo"),
            _ => panic!("expected Unknown command"),
        }
    }

    #[test]
    fn test_parse_regular_prompt() {
        match parse_input("explain this code") {
            Command::Prompt(text) => assert_eq!(text, "explain this code"),
            _ => panic!("expected Prompt command"),
        }
    }

    #[test]
    fn test_parse_empty() {
        assert!(matches!(parse_input(""), Command::Empty));
        assert!(matches!(parse_input("   "), Command::Empty));
    }

    #[test]
    fn test_session_clear_history() {
        let config = Config::default();
        let mut session = Session::new(config);

        // Add a fake user message
        session.messages.push(Message {
            role: Role::User,
            content: "test".to_string(),
            tool_calls: None,
            tool_call_id: None,
        });
        assert_eq!(session.messages.len(), 2);

        session.clear_history();
        assert_eq!(session.messages.len(), 1);
        assert!(matches!(session.messages[0].role, Role::System));
    }

    #[test]
    fn test_session_switch_model() {
        let config = Config::default();
        let mut session = Session::new(config);

        assert_eq!(session.config.model, "llama3.2");
        session.switch_model("qwen2.5-coder");
        assert_eq!(session.config.model, "qwen2.5-coder");
    }
}
