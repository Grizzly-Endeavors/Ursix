mod event;
mod state;
mod widgets;

#[cfg(test)]
pub(crate) mod test_helpers;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
};
use tokio::sync::mpsc;

use crate::agent::{Agent, AgentEvent};
use crate::config::Config;
use crate::llm::ollama::OllamaClient;

use event::{EventHandler, TuiEvent};
use state::AppState;
use widgets::{InputBox, MessageList, StatusBar};

/// Tick rate for animations (100ms = 10 fps)
const TICK_RATE: Duration = Duration::from_millis(100);

/// The main TUI application
pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    state: AppState,
    event_handler: EventHandler,
}

impl TuiApp {
    /// Create a new TUI application
    ///
    /// # Errors
    /// Returns error if terminal initialization fails
    pub fn new(config: Config) -> Result<Self> {
        let terminal = setup_terminal().context("failed to setup terminal")?;
        let state = AppState::new(config);
        let event_handler = EventHandler::new(TICK_RATE);

        Ok(Self {
            terminal,
            state,
            event_handler,
        })
    }

    /// Run the TUI application main loop
    ///
    /// # Errors
    /// Returns error if terminal operations fail
    pub async fn run(&mut self) -> Result<()> {
        // Install panic hook to restore terminal on panic
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            restore_terminal_force();
            original_hook(panic_info);
        }));

        let result = self.main_loop().await;

        // Restore terminal
        restore_terminal(&mut self.terminal).context("failed to restore terminal")?;

        result
    }

    async fn main_loop(&mut self) -> Result<()> {
        loop {
            // Render current state
            self.render()?;

            // Handle next event
            let Some(event) = self.event_handler.next().await else {
                break;
            };

            match event {
                TuiEvent::Input(key) => {
                    if self.handle_key(key).await? {
                        break;
                    }
                }
                TuiEvent::Agent(agent_event) => {
                    self.state.handle_agent_event(agent_event);
                }
                TuiEvent::Tick => {
                    self.state.tick();
                }
                TuiEvent::Resize { .. } => {
                    // Terminal handles resize automatically
                }
            }
        }

        Ok(())
    }

    fn render(&mut self) -> Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.area();

            // Layout: status bar (1 line), messages (flexible), input (3 lines)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // Status bar
                    Constraint::Min(5),    // Messages
                    Constraint::Length(3), // Input
                ])
                .split(area);

            // Render widgets
            frame.render_widget(StatusBar::new(&self.state), chunks[0]);
            frame.render_widget(MessageList::new(&self.state), chunks[1]);
            frame.render_widget(InputBox::new(&self.state), chunks[2]);
        })?;

        Ok(())
    }

    /// Handle a key press event
    ///
    /// Returns true if the application should quit
    async fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Global key bindings (always available)
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                return Ok(true); // Quit
            }
            (KeyModifiers::NONE, KeyCode::Up) => {
                self.state.scroll_up();
                return Ok(false);
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                let max_scroll = self.state.messages.len().saturating_mul(5);
                self.state.scroll_down(max_scroll);
                return Ok(false);
            }
            _ => {}
        }

        // Input handling (disabled while agent is running)
        if self.state.is_agent_running() {
            return Ok(false);
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Enter) => {
                self.submit_input().await?;
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                self.state.delete_char();
            }
            (KeyModifiers::NONE, KeyCode::Left) => {
                self.state.move_cursor_left();
            }
            (KeyModifiers::NONE, KeyCode::Right) => {
                self.state.move_cursor_right();
            }
            (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                self.state.move_cursor_start();
            }
            (KeyModifiers::NONE, KeyCode::End) | (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
                self.state.move_cursor_end();
            }
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.state.clear_input();
            }
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                self.state.insert_char(c);
            }
            _ => {}
        }

        Ok(false)
    }

    async fn submit_input(&mut self) -> Result<()> {
        let input = self.state.take_input();
        if input.is_empty() {
            return Ok(());
        }

        // Handle commands
        if input.starts_with('/') {
            self.handle_command(&input);
            return Ok(());
        }

        // Add user message to display
        self.state.add_user_message(input.clone());

        // Create agent and run with events
        let config = self.state.config.clone();
        let client = OllamaClient::new(&config.ollama_url, &config.model);
        let agent = Agent::new(config, client);

        // Get event sender for forwarding agent events
        let tui_tx = self.event_handler.agent_event_sender();

        // Create channel for agent events
        let (agent_tx, mut agent_rx) = mpsc::unbounded_channel::<AgentEvent>();

        // Forward agent events to TUI event channel
        tokio::spawn(async move {
            while let Some(event) = agent_rx.recv().await {
                if tui_tx.send(TuiEvent::Agent(event)).is_err() {
                    break;
                }
            }
        });

        // Run agent in background task
        let mut history = self.state.history.clone();
        let prompt = input;

        tokio::spawn(async move {
            let result = agent
                .run_with_events(&mut history, &prompt, Some(agent_tx))
                .await;
            if let Err(e) = result {
                tracing::error!(error = %e, "agent execution failed");
            }
            // Note: history is consumed here, we'll rebuild from messages
        });

        Ok(())
    }

    fn handle_command(&mut self, input: &str) {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts.first().map(|s| s.to_lowercase());

        match command.as_deref() {
            Some("/quit" | "/q" | "/exit") => {
                // Signal quit - this is a bit awkward, we'll handle it differently
                // For now, just ignore and let Ctrl+C handle quit
            }
            Some("/clear") => {
                self.state.clear_conversation();
            }
            Some("/model") => {
                if let Some(model) = parts.get(1) {
                    self.state.config.model = (*model).to_string();
                }
            }
            Some("/help" | "/?") => {
                // Add help message
                self.state.messages.push(state::DisplayMessage::Assistant {
                    content: "Available commands:\n  /clear - Clear conversation\n  /model <name> - Switch model\n  /quit - Exit\n  /help - Show this help".to_string(),
                    tool_executions: vec![],
                });
            }
            _ => {
                self.state
                    .messages
                    .push(state::DisplayMessage::Error(format!(
                        "Unknown command: {input}"
                    )));
            }
        }
    }
}

/// Setup the terminal for TUI mode
fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).context("failed to create terminal")?;
    Ok(terminal)
}

/// Restore the terminal to normal mode
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    terminal.show_cursor().context("failed to show cursor")?;
    Ok(())
}

/// Force restore terminal (used in panic hook)
fn restore_terminal_force() {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen);
}

#[cfg(test)]
mod tests {
    // TUI tests would require mock terminal backend
    // Most logic is tested through state tests
}
