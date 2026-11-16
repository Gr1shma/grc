//! Context-sensitive key hint line.

use crate::tui::state::{AppState, Focus, Mode};
use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Draw the hint line for the current mode and panel.
pub fn draw(f: &mut Frame, app: &AppState, area: Rect, theme: &Theme) {
    let hints: &[(&str, &str)] = match &app.mode {
        Mode::Normal if app.focus == Focus::Left => &[
            ("j/k", "move"),
            ("l/Enter", "tasks"),
            ("a/o", "new"),
            ("i", "rename"),
            ("dd", "delete"),
            ("?", "help"),
        ],
        Mode::Normal => &[
            ("j/k", "move"),
            ("x", "toggle"),
            ("a/o", "add"),
            ("i", "edit"),
            ("t", "due"),
            ("?", "help"),
        ],
        Mode::Filter => &[("type", "filter"), ("Enter", "apply"), ("Esc", "clear")],
        Mode::Help => &[("j/k", "scroll"), ("Esc/q/?", "close")],
        Mode::Conflict => &[("r", "reload"), ("o", "overwrite")],
        Mode::InputTask(_) | Mode::InputSection(_) => &[
            ("Enter", "save"),
            ("Esc", "cancel"),
            ("←/→", "cursor"),
            ("Ctrl-a/e", "home/end"),
        ],
        Mode::InputDue(_) => &[("Enter", "save"), ("Esc", "cancel"), ("empty", "clears")],
    };

    let mut spans = vec![Span::raw(" ")];
    for (index, (key, description)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                format!("  {}  ", theme.glyphs.separator),
                theme.hint_style(),
            ));
        }
        spans.push(Span::styled((*key).to_string(), theme.key_style()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*description).to_string(), theme.hint_style()));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
