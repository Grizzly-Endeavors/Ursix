use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::state::{AgentStatus, AppState};

/// Status bar widget showing model info, status, and spinner
pub struct StatusBar<'a> {
    state: &'a AppState,
}

impl<'a> StatusBar<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        // Build status components
        let model_span = Span::styled(
            format!(" {} ", self.state.config.model),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let turn_info = if self.state.is_agent_running() {
            format!(
                " Turn {}/{} ",
                self.state.current_turn + 1,
                self.state.max_turns
            )
        } else {
            String::new()
        };

        let turn_span = Span::styled(turn_info, Style::default().fg(Color::DarkGray));

        let (status_text, status_style): (String, Style) = match &self.state.status {
            AgentStatus::Idle => ("Ready".to_string(), Style::default().fg(Color::Green)),
            AgentStatus::Thinking => (
                "Thinking...".to_string(),
                Style::default().fg(Color::Yellow),
            ),
            AgentStatus::ExecutingTool { name } => (
                format!("Running {name}..."),
                Style::default().fg(Color::Yellow),
            ),
            AgentStatus::Error(msg) => (format!("Error: {msg}"), Style::default().fg(Color::Red)),
        };

        let status_span = Span::styled(format!(" {status_text} "), status_style);

        // Spinner (only when running)
        let spinner_span = if self.state.is_agent_running() {
            Span::styled(
                format!(" {} ", self.state.spinner_char()),
                Style::default().fg(Color::Cyan),
            )
        } else {
            Span::raw("")
        };

        // Help hint on the right
        let help_span = Span::styled(
            " Tab:focus  Ctrl+C:quit ",
            Style::default().fg(Color::DarkGray),
        );

        // Calculate spacing
        let left_content = format!(
            "{}{}{}{}",
            model_span.content, turn_span.content, status_span.content, spinner_span.content
        );
        let right_content = help_span.content.to_string();
        let total_len = left_content.len() + right_content.len();
        let padding = if inner.width as usize > total_len {
            inner.width as usize - total_len
        } else {
            1
        };

        let line = Line::from(vec![
            model_span,
            turn_span,
            status_span,
            spinner_span,
            Span::raw(" ".repeat(padding)),
            help_span,
        ]);

        let paragraph = Paragraph::new(line).alignment(Alignment::Left);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn default_config() -> Config {
        Config {
            model: "llama3.2".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
            working_dir: std::path::PathBuf::from("/tmp"),
            max_turns: 10,
        }
    }

    #[test]
    fn test_status_bar_renders_model() {
        let state = AppState::new(default_config());

        let backend = TestBackend::new(80, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                let widget = StatusBar::new(&state);
                frame.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(content.contains("llama3.2"));
        assert!(content.contains("Ready"));
    }
}
