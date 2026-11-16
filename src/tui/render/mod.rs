use crate::task::TodoList;
use crate::tui::state::{AppState, MessageKind, Mode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::path::Path;

pub use help::draw_help_overlay;
pub use left::draw_left;
pub use right::draw_right;

mod help;
mod left;
mod right;

pub fn split_at_char(s: &str, char_idx: usize) -> (&str, &str) {
    crate::tui::input::text::split_at_char(s, char_idx)
}

pub fn draw_ui(f: &mut Frame, todo_list: &TodoList, app: &mut AppState, path: &Path) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(crate::tui::LEFT_PANEL_PERCENT),
            Constraint::Percentage(100 - crate::tui::LEFT_PANEL_PERCENT),
        ])
        .split(chunks[0]);

    draw_left(f, todo_list, app, panels[0]);
    draw_right(f, todo_list, app, panels[1]);

    draw_filter_bar(f, todo_list, app, path, chunks[1]);

    if matches!(app.mode, Mode::Help) {
        app.help_scroll = draw_help_overlay(f, app, area);
    }
}

fn draw_filter_bar(f: &mut Frame, todo_list: &TodoList, app: &AppState, path: &Path, area: Rect) {
    let (text, fg) = match &app.mode {
        Mode::Filter => (format!("/{}", app.filter), Color::Yellow),
        _ if !app.message.is_none() => {
            let message = app.message.as_ref().expect("checked above");
            let color = match message.kind {
                MessageKind::Info => Color::DarkGray,
                MessageKind::Success => Color::Green,
                MessageKind::Error => Color::Red,
            };
            (message.text.clone(), color)
        }
        _ if !todo_list.unsectioned_tasks.is_empty() => (
            format!(
                "{} orphan task(s) before the first heading are not shown",
                todo_list.unsectioned_tasks.len()
            ),
            Color::Yellow,
        ),
        _ if !app.filter.is_empty() => (
            format!(
                "filter: \"{}\"  (press / to edit, Esc to clear)",
                app.filter
            ),
            Color::DarkGray,
        ),
        _ => (String::new(), Color::Reset),
    };

    let right = format!(
        "{}/{}  {} ",
        todo_list.count_done_tasks(),
        todo_list.count_tasks(),
        path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned()
        )
    );
    let left_width = text.chars().count();
    let right_width = right.chars().count();
    let pad = " ".repeat(
        usize::from(area.width)
            .saturating_sub(left_width)
            .saturating_sub(right_width),
    );
    let line = Line::from(vec![
        Span::styled(text, Style::default().fg(fg)),
        Span::raw(pad),
        Span::styled(right, Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
