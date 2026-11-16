//! Small rendering widgets and shared display helpers.

pub mod hint;
pub mod popup;
pub mod status;

use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::symbols::border;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Rounded box-drawing border for terminals with the full Unicode box set.
pub const ROUNDED_BORDER: border::Set<'static> = border::ROUNDED;

/// `+ - |` border for `GRC_ASCII=1`.
pub const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

/// The border glyph set for the theme.
pub fn border_set(theme: &Theme) -> border::Set<'static> {
    if theme.ascii {
        ASCII_BORDER
    } else {
        ROUNDED_BORDER
    }
}

/// Blank cells between a panel's border and its content.
///
/// Shared by both panels so their first columns line up and so a selection bar
/// never touches the border.
pub const GUTTER: usize = 1;

/// A bordered panel with a themed title and focus-dependent border.
///
/// Centralising this keeps the rounded corners and title padding consistent
/// between the section tree, the task list, and the help dialog.
pub fn panel<'a>(theme: &Theme, title: &str, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(border_set(theme))
        .title(Span::styled(format!(" {title} "), theme.title(focused)))
        .border_style(theme.border(focused))
        .style(theme.panel_style())
}

/// Number of terminal cells `text` occupies.
///
/// Counting characters is wrong for anything outside Latin-1: a CJK ideograph
/// is two cells and a combining mark is none, so a row measured in characters
/// overflows its panel or leaves a ragged gap. `unicode-width` follows
/// Unicode's East Asian Width so the arithmetic matches the terminal.
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Cells one character occupies (0 for combining marks, 2 for wide glyphs).
fn char_width(character: char) -> usize {
    UnicodeWidthChar::width(character).unwrap_or(0)
}

/// Split `text` at a character index, clamping out-of-range values.
pub fn split_at_char(text: &str, char_idx: usize) -> (&str, &str) {
    let byte = text
        .char_indices()
        .nth(char_idx)
        .map_or(text.len(), |(index, _)| index);
    text.split_at(byte)
}

/// Keep the longest prefix of `text` that fits in `width` terminal cells.
///
/// A double-width character that would straddle the boundary is dropped rather
/// than half-drawn, so the result can be one cell narrower than `width`.
pub fn truncate(text: &str, width: usize) -> String {
    let mut used = 0;
    let mut end = 0;
    for (index, character) in text.char_indices() {
        let cells = char_width(character);
        if used + cells > width {
            break;
        }
        used += cells;
        end = index + character.len_utf8();
    }
    text[..end].to_string()
}

/// Truncate to `width` terminal cells, using `ellipsis` when text is removed.
pub fn truncate_ellipsis(text: &str, width: usize, ellipsis: &str) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    let marker = display_width(ellipsis);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_measured_in_terminal_cells() {
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("ascii"), 5);
        // A CJK ideograph is two cells wide.
        assert_eq!(display_width("日本語"), 6);
        assert_eq!(display_width("a日本"), 5);
        // A combining mark adds no cells of its own.
        assert_eq!(display_width("e\u{301}"), 1);
        assert_eq!(display_width("e\u{301}e\u{301}"), 2);
        // A pictograph with emoji presentation is two cells.
        assert_eq!(display_width("🦀"), 2);
    }

    #[test]
    fn truncation_never_splits_a_wide_character() {
        assert_eq!(truncate("日本語", 6), "日本語");
        assert_eq!(truncate("日本語", 5), "日本");
        assert_eq!(truncate("日本語", 3), "日");
        assert_eq!(truncate("日x", 2), "日");
        assert_eq!(truncate("x日", 1), "x");
        assert_eq!(truncate("日本語", 0), "");
    }

    #[test]
    fn truncation_keeps_a_combining_mark_with_its_base() {
        assert_eq!(truncate("e\u{301}xy", 1), "e\u{301}");
        assert_eq!(truncate("e\u{301}xy", 2), "e\u{301}x");
        assert_eq!(truncate("e\u{301}xy", 0), "");
    }

    #[test]
    fn ellipsis_truncation_counts_cells() {
        assert_eq!(truncate_ellipsis("日本語", 6, "…"), "日本語");
        assert_eq!(truncate_ellipsis("日本語", 5, "…"), "日本…");
        assert_eq!(truncate_ellipsis("日本語", 4, "…"), "日…");
        assert_eq!(truncate_ellipsis("日本語", 3, "…"), "日…");
        // No room for the marker itself: show what fits and no marker.
        assert_eq!(truncate_ellipsis("日本語", 2, "…"), "…");
        assert_eq!(truncate_ellipsis("日本語", 1, "…"), "");
        assert_eq!(truncate_ellipsis("日本語", 0, "…"), "");
        // The ASCII marker costs three cells, so it is dropped sooner.
        assert_eq!(truncate_ellipsis(" 영어", 3, "..."), " 영");
        assert_eq!(truncate_ellipsis("英語", 3, "..."), "英");
        assert_eq!(truncate_ellipsis("abcdef", 5, "..."), "ab...");
    }
}
