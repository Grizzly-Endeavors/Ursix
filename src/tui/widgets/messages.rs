use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::tui::state::{AppState, DisplayMessage, Focus};

use super::ToolOutput;

/// Message list widget displaying conversation history
pub struct MessageList<'a> {
    state: &'a AppState,
}

impl<'a> MessageList<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    fn render_message(
        &self,
        msg: &DisplayMessage,
        msg_idx: usize,
        width: u16,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let content_width = width.saturating_sub(4) as usize; // Account for borders and padding

        match msg {
            DisplayMessage::User(content) => {
                // User message header
                lines.push(Line::from(vec![
                    Span::styled(
                        "You",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(":"),
                ]));
                // Wrap content
                for line in wrap_text(content, content_width) {
                    lines.push(Line::from(Span::raw(format!("  {line}"))));
                }
                lines.push(Line::raw("")); // Empty line after message
            }
            DisplayMessage::Assistant {
                content,
                tool_executions,
            } => {
                // Assistant message header
                lines.push(Line::from(vec![
                    Span::styled(
                        "Assistant",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(":"),
                ]));

                // Wrap content if not empty
                if !content.is_empty() {
                    for line in wrap_text(content, content_width) {
                        lines.push(Line::from(Span::raw(format!("  {line}"))));
                    }
                }

                // Render tool executions
                for (tool_idx, tool) in tool_executions.iter().enumerate() {
                    let is_selected = self.state.selected_message == Some(msg_idx)
                        && self.state.selected_tool == Some(tool_idx);
                    let tool_lines = ToolOutput::render_lines(tool, is_selected, content_width);
                    lines.extend(tool_lines);
                }

                lines.push(Line::raw("")); // Empty line after message
            }
            DisplayMessage::Error(message) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Error",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(": "),
                    Span::styled(message.clone(), Style::default().fg(Color::Red)),
                ]));
                lines.push(Line::raw(""));
            }
        }

        lines
    }
}

impl Widget for MessageList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.focus == Focus::Messages;

        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Conversation ");

        let inner = block.inner(area);
        block.render(area, buf);

        if self.state.messages.is_empty() {
            // Show placeholder when empty
            let placeholder = Paragraph::new(Text::styled(
                "No messages yet. Type a message and press Enter.",
                Style::default().fg(Color::DarkGray),
            ));
            placeholder.render(inner, buf);
            return;
        }

        // Build all lines for messages
        let mut all_lines: Vec<Line<'static>> = Vec::new();
        for (idx, msg) in self.state.messages.iter().enumerate() {
            let msg_lines = self.render_message(msg, idx, inner.width);
            all_lines.extend(msg_lines);
        }

        // Apply scroll offset
        let visible_lines: Vec<Line<'_>> = all_lines
            .into_iter()
            .skip(self.state.scroll_offset)
            .take(inner.height as usize)
            .collect();

        let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
        paragraph.render(inner, buf);
    }
}

/// Wrap text to fit within the given width
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        for word in paragraph.split_whitespace() {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_text_short() {
        let result = wrap_text("hello world", 20);
        assert_eq!(result, vec!["hello world"]);
    }

    #[test]
    fn test_wrap_text_long() {
        let result = wrap_text("hello world this is a longer text", 10);
        assert_eq!(
            result,
            vec!["hello", "world this", "is a", "longer", "text"]
        );
    }

    #[test]
    fn test_wrap_text_newlines() {
        let result = wrap_text("line one\nline two", 20);
        assert_eq!(result, vec!["line one", "line two"]);
    }

    #[test]
    fn test_wrap_text_empty() {
        let result = wrap_text("", 20);
        assert_eq!(result, vec![""]);
    }
}
