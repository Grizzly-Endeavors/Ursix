use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::state::{AppState, Focus};

/// Input box widget for user text entry
pub struct InputBox<'a> {
    state: &'a AppState,
}

impl<'a> InputBox<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

impl Widget for InputBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.focus == Focus::Input;
        let is_disabled = self.state.is_agent_running();

        // Build border style based on state
        let border_style = if is_disabled {
            Style::default().fg(Color::DarkGray)
        } else if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };

        let title = if is_disabled {
            " Agent Running... "
        } else {
            " Input "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title);

        // Build input text with cursor
        let text_style = if is_disabled {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        let input_text = if is_focused && !is_disabled {
            // Show cursor
            let (before, after) = self.state.input.split_at(self.state.cursor_position);
            let cursor_char = after.chars().next().unwrap_or(' ');
            let after_cursor = if after.is_empty() {
                String::new()
            } else {
                after.chars().skip(1).collect()
            };

            Line::from(vec![
                Span::styled(before.to_string(), text_style),
                Span::styled(
                    cursor_char.to_string(),
                    text_style.add_modifier(Modifier::REVERSED),
                ),
                Span::styled(after_cursor, text_style),
            ])
        } else {
            Line::from(Span::styled(self.state.input.clone(), text_style))
        };

        let paragraph = Paragraph::new(input_text).block(block);
        paragraph.render(area, buf);
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
            model: "test".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
            working_dir: std::path::PathBuf::from("/tmp"),
            max_turns: 10,
        }
    }

    #[test]
    fn test_input_box_renders() {
        let mut state = AppState::new(default_config());
        state.input = "hello".to_string();
        state.cursor_position = 5;

        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                let widget = InputBox::new(&state);
                frame.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Check that the input text is rendered
        let content: String = buffer
            .content
            .iter()
            .take(40 * 2) // First two lines
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(content.contains("hello"));
    }
}
