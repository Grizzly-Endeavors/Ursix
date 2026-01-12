use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::tui::state::ToolExecution;

/// Maximum lines to show for tool output when expanded
const MAX_OUTPUT_LINES: usize = 10;

/// Widget for rendering tool execution output
pub struct ToolOutput;

impl ToolOutput {
    /// Render tool execution as lines of text
    pub fn render_lines(
        tool: &ToolExecution,
        is_selected: bool,
        max_width: usize,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        lines.push(Self::render_header(tool, is_selected));

        if tool.collapsed {
            return lines;
        }

        Self::render_arguments(tool, max_width, &mut lines);
        Self::render_output(tool, max_width, &mut lines);

        lines
    }

    fn render_header(tool: &ToolExecution, is_selected: bool) -> Line<'static> {
        let collapse_indicator = if tool.collapsed { "[+]" } else { "[-]" };

        let status_indicator = match &tool.result {
            Some(result) if result.success => {
                Span::styled(" ✓ ", Style::default().fg(Color::Green))
            }
            Some(_) => Span::styled(" ✗ ", Style::default().fg(Color::Red)),
            None => Span::styled(" ● ", Style::default().fg(Color::Yellow)),
        };

        let duration_text = tool
            .duration_ms
            .map(|ms| format!(" ({ms}ms)"))
            .unwrap_or_default();

        let header_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        Line::from(vec![
            Span::styled(format!("  {collapse_indicator} "), header_style),
            status_indicator,
            Span::styled(
                tool.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(duration_text, Style::default().fg(Color::DarkGray)),
        ])
    }

    fn render_arguments(tool: &ToolExecution, max_width: usize, lines: &mut Vec<Line<'static>>) {
        let args_str = serde_json::to_string_pretty(&tool.arguments)
            .unwrap_or_else(|_| tool.arguments.to_string());

        let args_preview: String = args_str
            .lines()
            .take(3)
            .map(|l| truncate_line(l, max_width.saturating_sub(6)))
            .collect::<Vec<_>>()
            .join("\n");

        lines.push(Line::from(Span::styled(
            format!("      Args: {}", args_preview.lines().next().unwrap_or("")),
            Style::default().fg(Color::DarkGray),
        )));

        for line in args_preview.lines().skip(1) {
            lines.push(Line::from(Span::styled(
                format!("            {line}"),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    fn render_output(tool: &ToolExecution, max_width: usize, lines: &mut Vec<Line<'static>>) {
        let Some(result) = &tool.result else {
            return;
        };

        let output_style = if result.success {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Red)
        };

        let output_text = if result.success {
            &result.output
        } else {
            result.error.as_deref().unwrap_or("Unknown error")
        };

        if output_text.is_empty() {
            return;
        }

        lines.push(Line::from(Span::styled(
            "      Output:",
            Style::default().fg(Color::DarkGray),
        )));

        let output_lines: Vec<&str> = output_text.lines().collect();
        let truncated = output_lines.len() > MAX_OUTPUT_LINES;
        let display_lines = if truncated {
            &output_lines[..MAX_OUTPUT_LINES]
        } else {
            &output_lines[..]
        };

        for line in display_lines {
            let truncated_line = truncate_line(line, max_width.saturating_sub(8));
            lines.push(Line::from(Span::styled(
                format!("        {truncated_line}"),
                output_style,
            )));
        }

        if truncated {
            lines.push(Line::from(Span::styled(
                format!(
                    "        ... ({} more lines)",
                    output_lines.len() - MAX_OUTPUT_LINES
                ),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
}

fn truncate_line(line: &str, max_len: usize) -> String {
    if line.len() > max_len {
        format!("{}...", &line[..max_len.saturating_sub(3)])
    } else {
        line.to_string()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tools::ToolResult;

    #[test]
    fn test_render_collapsed_tool() {
        let tool = ToolExecution {
            id: "call_0".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "echo hello"}),
            result: Some(ToolResult::success("hello")),
            duration_ms: Some(50),
            collapsed: true,
        };

        let lines = ToolOutput::render_lines(&tool, false, 80);
        assert_eq!(lines.len(), 1); // Only header when collapsed
    }

    #[test]
    fn test_render_expanded_tool() {
        let tool = ToolExecution {
            id: "call_0".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "echo hello"}),
            result: Some(ToolResult::success("hello")),
            duration_ms: Some(50),
            collapsed: false,
        };

        let lines = ToolOutput::render_lines(&tool, false, 80);
        assert!(lines.len() > 1); // Header + args + output
    }

    #[test]
    fn test_render_running_tool() {
        let tool = ToolExecution {
            id: "call_0".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "sleep 10"}),
            result: None,
            duration_ms: None,
            collapsed: false,
        };

        let lines = ToolOutput::render_lines(&tool, false, 80);
        // Should have header and args, but no output section
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_render_failed_tool() {
        let tool = ToolExecution {
            id: "call_0".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "false"}),
            result: Some(ToolResult::failure("command failed")),
            duration_ms: Some(10),
            collapsed: false,
        };

        let lines = ToolOutput::render_lines(&tool, false, 80);
        // Check that we have output showing the error
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(all_text.contains("command failed") || all_text.contains("Output"));
    }

    #[test]
    fn test_truncate_line() {
        assert_eq!(truncate_line("short", 10), "short");
        assert_eq!(truncate_line("this is a long line", 10), "this is...");
    }
}
