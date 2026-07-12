use crate::task::TodoList;
use crate::tui::state::{AppState, Mode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

pub use help::draw_help_overlay;
pub use left::draw_left;
pub use right::draw_right;

mod help;
mod left;
mod right;

pub(crate) fn split_at_char(s: &str, char_idx: usize) -> (&str, &str) {
    let byte = s
        .char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len());
    (&s[..byte], &s[byte..])
}

pub fn draw_ui(f: &mut Frame, todo_list: &TodoList, app: &mut AppState) {
    let area = f.area();

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(27), Constraint::Percentage(73)])
        .split(area);

    draw_left(f, todo_list, app, panels[0]);
    draw_right(f, todo_list, app, panels[1]);

    if matches!(app.mode, Mode::Help) {
        draw_help_overlay(f, area);
    }
}
