//! Top-level layout and chrome.

use crate::task::TodoList;
use crate::tui::state::{AppState, Mode};
use crate::tui::theme::Theme;
use crate::tui::widgets::{hint, status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use std::path::Path;

pub(crate) mod help;
mod left;
mod right;

/// Draw the whole interface from immutable state.
pub fn draw_ui(f: &mut Frame, todo_list: &TodoList, app: &AppState, theme: &Theme, path: &Path) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(crate::tui::LEFT_PANEL_PERCENT),
            Constraint::Percentage(100 - crate::tui::LEFT_PANEL_PERCENT),
        ])
        .split(chunks[0]);

    left::draw_left(f, todo_list, app, theme, panels[0]);
    right::draw_right(f, todo_list, app, theme, panels[1]);
    hint::draw(f, app, chunks[1], theme);
    status::draw(f, todo_list, app, path, chunks[2], theme);

    if matches!(app.mode, Mode::Help) {
        help::draw_help_overlay(f, app, theme, area);
    }
}
