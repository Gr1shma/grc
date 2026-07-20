use crate::task::TodoList;
use crate::tui::state::{AppState, Mode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub use help::draw_help_overlay;
pub use left::draw_left;
pub use right::draw_right;

mod help;
mod left;
mod right;

pub fn split_at_char(s: &str, char_idx: usize) -> (&str, &str) {
    let byte = s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b);
    (&s[..byte], &s[byte..])
}

pub fn draw_ui(f: &mut Frame, todo_list: &TodoList, app: &mut AppState) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(27), Constraint::Percentage(73)])
        .split(chunks[0]);

    draw_left(f, todo_list, app, panels[0]);
    draw_right(f, todo_list, app, panels[1]);

    draw_filter_bar(f, app, chunks[1]);

    if matches!(app.mode, Mode::Help) {
        draw_help_overlay(f, app, area);
    }
}

fn draw_filter_bar(f: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let (text, fg) = match &app.mode {
        Mode::Filter => (format!("/{}│", app.filter), Color::Yellow),
        _ if !app.filter.is_empty() => (
            format!(
                "filter: \"{}\"  (press / to edit, Esc to clear)",
                app.filter
            ),
            Color::DarkGray,
        ),
        _ => (String::new(), Color::Reset),
    };
    let p = Paragraph::new(Line::from(Span::styled(text, Style::default().fg(fg))));
    f.render_widget(p, area);
}
