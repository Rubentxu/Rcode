//! Sidebar view - session list

use opencode_core::{Session, SessionStatus};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::OpencodeTui;

/// Sidebar view widget
pub struct SidebarView {
    list_state: ListState,
}

impl SidebarView {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
        }
    }

    /// Handle selection navigation
    pub fn next(&mut self, app: &OpencodeTui) {
        let sessions = app.filtered_sessions();
        if sessions.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1).min(sessions.len() - 1),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self, app: &OpencodeTui) {
        let sessions = app.filtered_sessions();
        if sessions.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Get selected session ID if any
    pub fn selected_session(&self, app: &OpencodeTui) -> Option<Session> {
        self.list_state
            .selected()
            .and_then(|i| app.filtered_sessions().get(i).cloned())
            .map(|s| (**s).clone())
    }

    /// Reset selection when search changes
    pub fn reset_selection(&mut self) {
        self.list_state.select(None);
    }

    /// Render the sidebar
    pub fn render(&mut self, app: &OpencodeTui, area: Rect, buf: &mut Buffer) {
        // Sidebar width - fixed 30 columns
        let sidebar_width = 30.min(area.width);
        let sidebar_area = Rect::new(area.x, area.y, sidebar_width, area.height);

        // Calculate layout
        let search_height = 3;
        let hint_height = 6;
        let list_height = sidebar_area
            .height
            .saturating_sub(search_height)
            .saturating_sub(hint_height)
            .saturating_sub(2); // borders

        let search_area = Rect::new(sidebar_area.x, sidebar_area.y, sidebar_width, search_height);
        let list_area = Rect::new(
            sidebar_area.x,
            sidebar_area.y + search_height,
            sidebar_width,
            list_height,
        );
        let hint_area = Rect::new(
            sidebar_area.x,
            sidebar_area.y + search_height + list_height,
            sidebar_width,
            hint_height,
        );

        // Search input
        let search_block = Block::default()
            .title(" Search ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        let search_para = Paragraph::new(app.session_search.as_str())
            .block(search_block)
            .style(Style::default().fg(Color::White));
        search_para.render(search_area, buf);

        // Session list
        let list_block = Block::default()
            .title(" Sessions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let list_items: Vec<ListItem> = app
            .filtered_sessions()
            .iter()
            .enumerate()
            .map(|(i, session)| {
                let title = session
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("Session {}", &session.id.0[..8]));
                let status_icon = match session.status {
                    SessionStatus::Idle => "○",
                    SessionStatus::Running => "◉",
                    SessionStatus::Completed => "✓",
                    SessionStatus::Aborted => "✗",
                };
                let content = Text::raw(format!("{} {}", status_icon, title));
                let style = if Some(i) == self.list_state.selected() {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(content).style(style)
            })
            .collect();

        let session_list = List::new(list_items).block(list_block).highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        ratatui::prelude::StatefulWidget::render(
            &session_list,
            list_area,
            buf,
            &mut self.list_state,
        );

        // New session hint
        let hint_block = Block::default()
            .title(" Actions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));
        let hint_text = Paragraph::new("Ctrl+N: New session\nCtrl+S: Switch\nCtrl+Q: Quit")
            .block(hint_block)
            .style(Style::default().fg(Color::Gray));
        hint_text.render(hint_area, buf);
    }
}

impl Default for SidebarView {
    fn default() -> Self {
        Self::new()
    }
}
