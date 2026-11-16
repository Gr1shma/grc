//! Small rendering widgets and shared display helpers.

pub mod hint;
pub mod popup;
pub mod status;

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;

/// Number of terminal cells a string occupies in the common case.
///
/// The UI uses single-width Latin glyphs for its chrome; task text is measured
/// in characters, which is exact for the ASCII-heavy Markdown this app targets.
pub fn display_width(text: &str) -> usize {
    text.chars().count()
}

/// Split `text` at a character index, clamping out-of-range values.
pub fn split_at_char(text: &str, char_idx: usize) -> (&str, &str) {
    let byte = text
        .char_indices()
        .nth(char_idx)
        .map_or(text.len(), |(index, _)| index);
    text.split_at(byte)
}

/// Keep at most `width` characters.
pub fn truncate(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

/// Truncate to `width` characters, using `ellipsis` when text is removed.
pub fn truncate_ellipsis(text: &str, width: usize, ellipsis: &str) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let marker = ellipsis.chars().count();
    if width <= marker {
        return truncate(text, width);
    }
    format!("{}{}", truncate(text, width - marker), ellipsis)
}

/// Build spans for `text`, emphasising the first case-insensitive occurrence of
/// `needle`. If the match cannot be sliced safely, the text is returned plain.
pub fn highlight(text: &str, needle: &str, base: Style, emphasis: Style) -> Vec<Span<'static>> {
    if needle.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }

    let Some(start) = text.to_lowercase().find(needle) else {
        return vec![Span::styled(text.to_string(), base)];
    };
    let end = start + needle.len();
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return vec![Span::styled(text.to_string(), base)];
    }

    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::styled(text[..start].to_string(), base));
    }
    spans.push(Span::styled(text[start..end].to_string(), emphasis));
    if end < text.len() {
        spans.push(Span::styled(text[end..].to_string(), base));
    }
    spans
}

/// Minimal scroll offset that keeps `selected` inside a viewport.
pub fn list_offset(total: usize, selected: usize, height: usize) -> usize {
    if total <= height || height == 0 {
        return 0;
    }
    let selected = selected.min(total - 1);
    if selected < height {
        0
    } else {
        (selected + 1 - height).min(total - height)
    }
}

/// Width of a percentage of a terminal dimension without overflow.
pub const fn percentage(total: u16, percent: u16) -> u16 {
    (total as u32 * percent as u32 / 100) as u16
}

/// One-cell vertical scrollbar strip on the right border of a bordered panel.
///
/// Rendering the scrollbar inside the border keeps the rounded corners intact.
pub const fn border_scrollbar(area: Rect) -> Rect {
    if area.height < 3 || area.width == 0 {
        return Rect::new(area.x, area.y, 0, 0);
    }
    Rect::new(area.x + area.width - 1, area.y + 1, 1, area.height - 2)
}
