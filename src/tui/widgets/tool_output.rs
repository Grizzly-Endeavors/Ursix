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
    #[allow(clippy::too_many_lines)]
    pub fn render_lines(
        tool: &ToolExecution,
        is_selected: bool,
        max_width: usize,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Build header line
        let collapse_indicator = if tool.collapsed { "[+]" } else { "[-]" };

        let status_indicator = match &tool.result {
            Some(result) if result.success => {
                Span::styled(" ✓ ", Style::default().fg(Color::Green))
            }
            Some(_) => Span::styled(" ✗ ", Style::default().fg(Color::Red)),
            None => Span::styled(" ● ", Style::default().fg(Color::Yellow)), // Running
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

        let header = Line::from(vec![
            Span::styled(format!("  {collapse_indicator} "), header_style),
            status_indicator,
            Span::styled(
                tool.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(duration_text, Style::default().fg(Color::DarkGray)),
        ]);
        lines.push(header);

        // If collapsed, don't show details
        if tool.collapsed {
            return lines;
        }

        // Show arguments (truncated)
        let args_str = serde_json::to_string_pretty(&tool.arguments)
            .unwrap_or_else(|_| tool.arguments.to_string());
        let args_preview: String = args_str
            .lines()
            .take(3)
            .map(|l| {
                if l.len() > max_width.saturating_sub(6) {
                    format!("{}...", &l[..max_width.saturating_sub(9)])
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        lines.push(Line::from(Span::styled(
            format!("      Args: {}", args_preview.lines().next().unwrap_or("")),
            Style::default().fg(Color::DarkGray),
        )));

        // Show more arg lines if present
        for line in args_preview.lines().skip(1) {
            lines.push(Line::from(Span::styled(
                format!("            {line}"),
                Style::default().fg(Color::DarkGray),
            )));
        }

        // Show output if completed
        if let Some(result) = &tool.result {
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

            if !output_text.is_empty() {
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
                    let truncated_line = if line.len() > max_width.saturating_sub(8) {
                        format!("{}...", &line[..max_width.saturating_sub(11)])
                    } else {
                        line.to_string()
                    };
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

        lines
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
}
