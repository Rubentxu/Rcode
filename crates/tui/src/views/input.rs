//! Input view - text input area

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use crate::app::OpencodeTui;

/// Input view widget for multi-line text input
pub struct InputView {
    cursor_position: usize,
}

impl InputView {
    pub fn new() -> Self {
        Self { cursor_position: 0 }
    }

    /// Handle character input
    pub fn insert_char(&mut self, c: char, app: &mut OpencodeTui) {
        app.input_buffer.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    /// Handle backspace
    pub fn backspace(&mut self, app: &mut OpencodeTui) {
        if self.cursor_position > 0 && !app.input_buffer.is_empty() {
            app.input_buffer.remove(self.cursor_position - 1);
            self.cursor_position -= 1;
        }
    }

    /// Handle delete
    pub fn delete(&mut self, app: &mut OpencodeTui) {
        if self.cursor_position < app.input_buffer.len() {
            app.input_buffer.remove(self.cursor_position);
        }
    }

    /// Move cursor left
    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    /// Move cursor right
    pub fn move_cursor_right(&mut self, app: &OpencodeTui) {
        if self.cursor_position < app.input_buffer.len() {
            self.cursor_position += 1;
        }
    }

    /// Move cursor to start
    pub fn move_cursor_to_start(&mut self) {
        self.cursor_position = 0;
    }

    /// Move cursor to end
    pub fn move_cursor_to_end(&mut self, app: &OpencodeTui) {
        self.cursor_position = app.input_buffer.len();
    }

    /// Clear input
    pub fn clear(&mut self, app: &mut OpencodeTui) {
        app.input_buffer.clear();
        self.cursor_position = 0;
    }

    /// Get the message to send (and clear input)
    pub fn take_message(&mut self, app: &mut OpencodeTui) -> Option<String> {
        if app.input_buffer.trim().is_empty() {
            return None;
        }
        let msg = app.input_buffer.clone();
        self.clear(app);
        Some(msg)
    }

    /// Render the input area
    pub fn render(&self, app: &OpencodeTui, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Message Input ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let inner = block.inner(area);
        block.render(area, buf);

        // Show input with cursor indicator
        let display_text = if app.input_buffer.is_empty() {
            String::new()
        } else {
            let (before, after) = app.input_buffer.split_at(self.cursor_position);
            format!("{}{}_", before, after)
        };

        let placeholder = if app.input_buffer.is_empty() {
            "Type your message and press Enter to send..."
        } else {
            ""
        };

        let text = if display_text.is_empty() {
            Span::styled(placeholder, Style::default().fg(Color::DarkGray))
        } else {
            Span::raw(display_text)
        };

        let para = Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .scroll((0, 0));
        para.render(inner, buf);
    }
}

impl Default for InputView {
    fn default() -> Self {
        Self::new()
    }
}
