//! Centered popup geometry and dialog chrome.

use crate::tui::theme::Theme;
use crate::tui::widgets::{BORDER_TYPE, percentage};
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders};

/// Center a popup with a percentage width and fixed height, clamped to `area`.
pub fn centered(area: Rect, width_percent: u16, height: u16) -> Rect {
    let width = percentage(area.width, width_percent).min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// Rounded dialog block with an accented title.
pub fn dialog<'a>(theme: &Theme, title: impl Into<String>) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BORDER_TYPE)
        .title(Span::styled(
            format!(" {} ", title.into()),
            theme.dialog_title(),
        ))
        .border_style(theme.dialog_border())
        .style(theme.base_style())
}
