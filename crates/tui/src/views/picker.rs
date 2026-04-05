//! Model picker view

use crate::app::RcodeTui;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

/// Model picker view widget
pub struct ModelPickerView {
    list_state: ListState,
}

impl ModelPickerView {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
        }
    }

    /// Get unique providers from model list
    fn get_providers(app: &RcodeTui) -> Vec<String> {
        let mut providers: Vec<String> = app
            .model_list
            .iter()
            .map(|(_, provider, _)| provider.clone())
            .collect();
        providers.sort();
        providers.dedup();
        providers
    }

    /// Get models filtered by current provider
    fn get_filtered_models(app: &RcodeTui) -> Vec<(String, String, bool)> {
        let providers = Self::get_providers(app);
        if providers.is_empty() {
            return app.model_list.clone();
        }
        let current_provider = providers
            .get(app.model_picker_provider_index)
            .cloned()
            .unwrap_or_default();
        app.model_list
            .iter()
            .filter(|(_, provider, _)| provider == &current_provider)
            .cloned()
            .collect()
    }

    /// Navigate down in the list
    pub fn next(&mut self, app: &mut RcodeTui) {
        let filtered = Self::get_filtered_models(app);
        if filtered.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1).min(filtered.len() - 1),
            None => 0,
        };
        self.list_state.select(Some(i));
        app.model_picker_index = i;
    }

    /// Navigate up in the list
    pub fn previous(&mut self, app: &mut RcodeTui) {
        let filtered = Self::get_filtered_models(app);
        if filtered.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.list_state.select(Some(i));
        app.model_picker_index = i;
    }

    /// Switch to next provider
    pub fn next_provider(&mut self, app: &mut RcodeTui) {
        let providers = Self::get_providers(app);
        if providers.is_empty() {
            return;
        }
        app.model_picker_provider_index = (app.model_picker_provider_index + 1) % providers.len();
        app.model_picker_index = 0;
        self.list_state.select(Some(0));
    }

    /// Switch to previous provider
    pub fn previous_provider(&mut self, app: &mut RcodeTui) {
        let providers = Self::get_providers(app);
        if providers.is_empty() {
            return;
        }
        app.model_picker_provider_index = app.model_picker_provider_index.saturating_sub(1);
        if app.model_picker_provider_index >= providers.len() {
            app.model_picker_provider_index = providers.len().saturating_sub(1);
        }
        app.model_picker_index = 0;
        self.list_state.select(Some(0));
    }

    /// Get selected model info
    pub fn selected_model(&self, app: &RcodeTui) -> Option<(String, String, bool)> {
        let filtered = Self::get_filtered_models(app);
        self.list_state
            .selected()
            .and_then(|i| filtered.get(i).cloned())
    }

    /// Reset selection
    pub fn reset(&mut self) {
        self.list_state.select(None);
    }

    /// Initialize picker state
    pub fn init(&mut self, app: &mut RcodeTui) {
        let providers = Self::get_providers(app);
        if !providers.is_empty() {
            app.model_picker_provider_index = 0;
            app.model_picker_index = 0;
            self.list_state.select(Some(0));
        }
    }

    /// Render the model picker
    pub fn render(&mut self, app: &mut RcodeTui, area: Rect, buf: &mut Buffer) {
        if area.width < 40 || area.height < 10 {
            // Too small to render properly
            return;
        }

        let providers = Self::get_providers(app);
        let filtered_models = Self::get_filtered_models(app);
        let current_provider = providers
            .get(app.model_picker_provider_index)
            .cloned()
            .unwrap_or_else(|| "All".to_string());

        // Calculate layout
        let provider_height = 3;
        let hint_height = 3;
        let list_height = area
            .height
            .saturating_sub(provider_height)
            .saturating_sub(hint_height)
            .saturating_sub(4); // borders and padding

        let provider_area = Rect::new(area.x + 1, area.y + 1, area.width - 2, provider_height);
        let list_area = Rect::new(
            area.x + 1,
            area.y + provider_height + 2,
            area.width - 2,
            list_height,
        );
        let hint_area = Rect::new(
            area.x + 1,
            area.y + area.height - hint_height - 2,
            area.width - 2,
            hint_height,
        );

        // Main border
        let outer_block = Block::default()
            .title(" Select Model ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        outer_block.render(area, buf);

        // Provider selector
        let provider_block = Block::default()
            .title(" Provider ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        let provider_text = if providers.len() > 1 {
            format!("{}  [<] {:.} [>]", current_provider, providers.len())
        } else {
            current_provider.clone()
        };
        let provider_para = Paragraph::new(provider_text)
            .block(provider_block)
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center);
        provider_para.render(provider_area, buf);

        // Model list
        let list_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let list_items: Vec<ListItem> = filtered_models
            .iter()
            .enumerate()
            .map(|(i, (model_id, _provider, enabled))| {
                let check = if *enabled { "✓" } else { " " };
                let content = Text::raw(format!("{} {}", check, model_id));
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

        let model_list = List::new(list_items).block(list_block).highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        ratatui::prelude::StatefulWidget::render(&model_list, list_area, buf, &mut self.list_state);

        // Hints
        let hint_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let hint_text = Paragraph::new("↑↓ Navigate  ←→ Provider  Enter Select  Esc Cancel")
            .block(hint_block)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center);
        hint_text.render(hint_area, buf);
    }
}

impl Default for ModelPickerView {
    fn default() -> Self {
        Self::new()
    }
}
