//! Chat view - message list with scroll

use opencode_core::{Message, Part, Role};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::{AppMode, OpencodeTui, ToolStatus};

/// Chat view widget
pub struct ChatView {
    scroll_offset: usize,
}

impl ChatView {
    pub fn new() -> Self {
        Self { scroll_offset: 0 }
    }

    /// Scroll up in message list
    pub fn scroll_up(&mut self, app: &OpencodeTui) {
        let max_scroll = app.messages.len().saturating_sub(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(1).min(max_scroll);
    }

    /// Scroll down in message list
    pub fn scroll_down(&mut self, app: &OpencodeTui) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    /// Page up
    pub fn page_up(&mut self, app: &OpencodeTui) {
        let page_size = 10;
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    /// Page down
    pub fn page_down(&mut self, app: &OpencodeTui) {
        let page_size = 10;
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(page_size)
            .min(app.messages.len().saturating_sub(1));
    }

    /// Reset scroll to bottom (most recent messages)
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    /// Render the chat view
    pub fn render(&mut self, app: &OpencodeTui, area: Rect, buf: &mut Buffer) {
        if app.current_session.is_none() {
            // No session selected - show welcome
            let welcome = Paragraph::new(
                "Welcome to OpenCode Rust TUI\n\n\
                 Select a session from the sidebar or create a new one.\n\n\
                 Controls:\n\
                 • Ctrl+N: New session\n\
                 • Ctrl+S: Switch to selected session\n\
                 • Ctrl+Q: Quit\n\
                 • ↑/↓: Navigate session list\n\
                 • PageUp/PageDown: Scroll chat",
            )
            .block(
                Block::default()
                    .title(" Welcome ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(Alignment::Center)
            .wrap(ratatui::widgets::Wrap { trim: true });
            welcome.render(area, buf);
            return;
        }

        // Calculate layout: messages take most space, input at bottom
        let input_height = 5;
        let status_height = if app.is_running || !app.tool_status.is_empty() {
            3
        } else {
            0
        };
        let messages_height = area
            .height
            .saturating_sub(input_height)
            .saturating_sub(status_height);

        let messages_area = Rect::new(area.x, area.y, area.width, messages_height);
        let status_area = Rect::new(area.x, area.y + messages_height, area.width, status_height);
        let input_area = Rect::new(
            area.x,
            area.y + messages_height + status_height,
            area.width,
            input_height,
        );

        // Render messages
        self.render_messages(app, messages_area, buf);

        // Render tool/status bar if active
        if status_height > 0 {
            self.render_status(app, status_area, buf);
        }

        // Render input hint (actual input handled separately)
        self.render_input_hint(app, input_area, buf);
    }

    fn render_messages(&mut self, app: &OpencodeTui, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(
                " Chat - {} ",
                app.current_session()
                    .and_then(|s| s.title.clone())
                    .unwrap_or_else(|| "Session".to_string())
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = block.inner(area);
        block.render(area, buf);

        if app.messages.is_empty() {
            let empty = Paragraph::new("No messages yet. Type a message below.")
                .style(Style::default().fg(Color::Gray))
                .alignment(Alignment::Center);
            empty.render(inner_area, buf);
            return;
        }

        // Show messages from scroll_offset onwards, limited to visible area
        let visible_messages: Vec<Message> = app
            .messages
            .iter()
            .skip(self.scroll_offset)
            .take(inner_area.height as usize)
            .cloned()
            .collect();

        let content: Vec<Line> = visible_messages
            .iter()
            .flat_map(|msg| self.format_message(msg))
            .collect();

        let paragraph = Paragraph::new(content)
            .scroll((self.scroll_offset as u16, 0))
            .style(Style::default().fg(Color::White));

        paragraph.render(inner_area, buf);
    }

    fn format_message(&self, msg: &Message) -> Vec<Line> {
        let role_label = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => "System",
        };
        let role_color = match msg.role {
            Role::User => Color::Green,
            Role::Assistant => Color::Blue,
            Role::System => Color::Yellow,
        };

        let mut lines = vec![Line::from(Span::styled(
            format!("[{}]", role_label),
            Style::default().fg(role_color).add_modifier(Modifier::BOLD),
        ))];

        for part in &msg.parts {
            match part {
                Part::Text { content } => {
                    for line in content.lines() {
                        lines.push(Line::from(Span::raw(format!("  {}", line))));
                    }
                }
                Part::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    let args_str = serde_json::to_string_pretty(arguments).unwrap_or_default();
                    lines.push(Line::from(Span::styled(
                        format!("  🔧 Tool: {} (id: {})", name, id),
                        Style::default().fg(Color::Magenta),
                    )));
                    lines.push(Line::from(Span::raw(format!("  Args: {}", args_str))));
                }
                Part::ToolResult {
                    tool_call_id,
                    content,
                    is_error,
                } => {
                    let style = if *is_error {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::Cyan)
                    };
                    lines.push(Line::from(Span::styled(
                        format!("  ✓ Result for {}: {}", tool_call_id, content),
                        style,
                    )));
                }
                Part::Reasoning { content } => {
                    lines.push(Line::from(Span::styled(
                        format!("  🤖 Reasoning: {}", content),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                Part::Attachment {
                    id,
                    name,
                    mime_type,
                    ..
                } => {
                    lines.push(Line::from(Span::styled(
                        format!("  📎 Attachment: {} ({})", name, mime_type),
                        Style::default().fg(Color::Yellow),
                    )));
                }
            }
        }

        lines.push(Line::from(Span::raw("")));
        lines
    }

    fn render_status(&self, app: &OpencodeTui, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Status ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        block.render(area, buf);

        let mut status_lines = Vec::new();

        if app.is_running {
            status_lines.push(Line::from(Span::styled(
                "● Agent is running...",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        for (tool_name, status) in &app.tool_status {
            let (icon, style, text) = match status {
                ToolStatus::Running => ("◉", Color::Yellow, "running..."),
                ToolStatus::Completed => ("✓", Color::Green, "completed"),
                ToolStatus::Failed(err) => ("✗", Color::Red, err.as_str()),
            };
            status_lines.push(Line::from(Span::styled(
                format!("{} Tool {}: {}", icon, tool_name, text),
                style,
            )));
        }

        if status_lines.is_empty() {
            status_lines.push(Line::from(Span::styled(
                "● Ready",
                Style::default().fg(Color::Gray),
            )));
        }

        let para = Paragraph::new(status_lines)
            .style(Style::default().fg(Color::White))
            .scroll((0, 0));
        para.render(inner, buf);
    }

    fn render_input_hint(&self, app: &OpencodeTui, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Input ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let inner = block.inner(area);
        block.render(area, buf);

        let input_text = if app.input_buffer.is_empty() {
            "Type your message... (Enter to send, Esc to cancel)"
        } else {
            app.input_buffer.as_str()
        };

        let para = Paragraph::new(input_text)
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Left);
        para.render(inner, buf);
    }
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}
