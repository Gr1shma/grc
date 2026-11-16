//! Scrollable keybinding overlay.

use crate::tui::state::AppState;
use crate::tui::theme::Theme;
use crate::tui::widgets::popup;
use crate::tui::{HELP_POPUP_HEIGHT, HELP_POPUP_WIDTH_PERCENT};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

/// Draw the help overlay. Scrolling is clamped only for display; input state
/// owns the stored offset so rendering never mutates application state.
pub fn draw_help_overlay(f: &mut Frame, app: &AppState, theme: &Theme, area: Rect) {
    let popup_area = popup::centered(area, HELP_POPUP_WIDTH_PERCENT, HELP_POPUP_HEIGHT);
    f.render_widget(Clear, popup_area);
    let content = help_content(theme);
    let visible = usize::from(popup_area.height.saturating_sub(2));
    let max_scroll = content.len().saturating_sub(visible);
    let scroll = usize::from(app.help_scroll).min(max_scroll) as u16;
    let paragraph = Paragraph::new(content)
        .block(popup::dialog(theme, "Keybindings"))
        .scroll((scroll, 0));
    f.render_widget(paragraph, popup_area);
}

/// Maximum useful stored scroll offset for a viewport height.
pub fn max_scroll(viewport_height: u16) -> u16 {
    let popup_height = HELP_POPUP_HEIGHT.min(viewport_height);
    let visible = popup_height.saturating_sub(2);
    let content = help_content(&Theme::plain()).len();
    u16::try_from(content.saturating_sub(usize::from(visible))).unwrap_or(u16::MAX)
}

fn help_content(theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    group(
        &mut lines,
        theme,
        "Navigate",
        &[
            ("j/k, ↓/↑", "move or scroll"),
            ("gg / G", "top / bottom"),
            ("5j / 3k", "count motion"),
            ("l/Enter", "focus tasks"),
            ("h/Esc/Tab", "back to sections"),
            ("J / K", "move task/section"),
            ("za / zM / zR", "fold / fold all / unfold all"),
            ("]d", "next overdue"),
            ("H", "hide/show completed"),
            ("q / Ctrl-c", "quit"),
        ],
    );
    group(
        &mut lines,
        theme,
        "Edit",
        &[
            ("a / A", "new section below (level)"),
            ("o / O", "new section below/above"),
            ("i / dd", "rename / delete section"),
            ("yy / p / P", "yank / paste below / above"),
            ("a / A / o / O", "add task (A appends)"),
            ("i / t", "edit text / due date"),
            ("x / Space / Enter", "toggle task"),
            ("dd / yy / p / P", "delete / yank / paste task"),
            ("3dd", "delete three tasks"),
            ("u / Ctrl-r", "undo / redo"),
            (":", "sort section by due date"),
        ],
    );
    group(
        &mut lines,
        theme,
        "Dates",
        &[
            ("2025-12-31", "exact date"),
            ("today | tod | t", "today"),
            ("tomorrow | tmr", "tomorrow"),
            ("tmw | tom", "tomorrow"),
            ("next week | nextweek | nw", "7 days"),
            ("mon .. sun", "strictly next weekday"),
            ("this mon", "that weekday, today allowed"),
            ("next mon", "strictly future weekday"),
            ("+3d | -2w | +1m | +1y", "relative offset"),
            ("in 3 days | jun 15", "offset or month + day"),
            ("due:DATE", "inline in task text"),
        ],
    );
    group(
        &mut lines,
        theme,
        "Misc",
        &[
            ("/", "live filter (type, Enter, Esc)"),
            ("Esc", "clear filter / go back"),
            ("? / Esc / q", "open / close help"),
            ("←/→, Home/End", "move text cursor"),
            ("Ctrl-a / Ctrl-e", "start / end of input"),
            ("Backspace / Delete", "delete before / at cursor"),
            ("Enter / Esc", "confirm / cancel input"),
            ("r / o", "reload / overwrite on conflict"),
            ("wheel / click", "scroll / select a row"),
            ("NO_COLOR=1", "disable colour"),
            ("GRC_ASCII=1", "ASCII checkboxes and folds"),
        ],
    );
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  j/k or the mouse wheel scrolls this help.",
        theme.help_text(),
    )));
    lines
}

fn group(lines: &mut Vec<Line<'static>>, theme: &Theme, title: &str, entries: &[(&str, &str)]) {
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("  {title}"),
        theme.help_heading(),
    )));
    for (key, description) in entries {
        lines.push(Line::from(vec![
            Span::styled("    ", theme.help_text()),
            Span::styled(format!("{key:<20}"), theme.help_key()),
            Span::styled((*description).to_string(), theme.help_text()),
        ]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_has_all_four_groups() {
        let text: String = help_content(&Theme::plain())
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("");
        for heading in ["Navigate", "Edit", "Dates", "Misc"] {
            assert!(text.contains(heading), "missing {heading}");
        }
    }

    #[test]
    fn max_scroll_is_bounded() {
        assert!(max_scroll(40) < max_scroll(10));
        assert_eq!(max_scroll(0), max_scroll(1));
    }
}
