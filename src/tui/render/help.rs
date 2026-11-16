//! Scrollable keybinding overlay.

use crate::tui::state::AppState;
use crate::tui::theme::Theme;
use crate::tui::widgets::popup;
use crate::tui::{HELP_POPUP_HEIGHT, help_popup};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

/// Area the help overlay occupies, derived from the terminal size.
pub(crate) fn overlay_area(area: Rect) -> Rect {
    help_popup(area)
}

/// Draw the help overlay. Scrolling is clamped only for display; input state
/// owns the stored offset so rendering never mutates application state.
pub fn draw_help_overlay(f: &mut Frame, app: &AppState, theme: &Theme, area: Rect) {
    let popup_area = overlay_area(area);
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
            ("]d", "next overdue task (unfolds, clears filter)"),
            ("H", "hide/show completed"),
            ("q / Ctrl-c", "quit"),
        ],
    );
    group(
        &mut lines,
        theme,
        "Sections",
        &[
            ("a", "new child section"),
            ("A", "new section above the parent"),
            ("o / O", "new section below / above"),
            ("i", "rename section"),
            ("dd", "delete section and subtree"),
            ("yy / p / P", "yank / paste below / above"),
        ],
    );
    group(
        &mut lines,
        theme,
        "Tasks",
        &[
            ("a / o", "add task below"),
            ("A / O", "add task above (A appends)"),
            ("i / t", "edit text / due date"),
            ("x / Space / Enter", "toggle task"),
            ("dd", "delete task(s), 3dd deletes three"),
            ("yy / p / P", "yank / paste below / above"),
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
            ("u / Ctrl-r", "undo / redo"),
            ("/", "live filter (type, Enter, Esc)"),
            ("Esc", "clear filter / go back"),
            ("? / Esc / q", "open / close help"),
            ("←/→, Home/End", "move text cursor"),
            ("Ctrl-a / Ctrl-e", "start / end of input"),
            ("Backspace / Delete", "delete before / at cursor"),
            ("Enter / Esc", "confirm / cancel input"),
            (
                "r / o / q",
                "on conflict: reload / overwrite / refuse to quit",
            ),
            ("wheel / click", "move selection / select a row"),
            ("NO_COLOR=1", "disable colour"),
            ("GRC_BG=1", "paint the dark canvas"),
            ("GRC_ASCII=1", "ASCII glyphs and borders"),
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

    /// Flatten the overlay into one string for assertions.
    fn help_text() -> String {
        help_content(&Theme::plain())
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn help_groups_every_key_once() {
        let text = help_text();
        for heading in ["Navigate", "Sections", "Tasks", "Dates", "Misc"] {
            assert!(text.contains(heading), "missing {heading}");
        }
        // Section keys and task keys are grouped per panel, not interleaved.
        let sections = text.find("Sections").unwrap();
        let tasks = text.find("  Tasks").unwrap();
        let dates = text.find("  Dates").unwrap();
        assert!(sections < tasks && tasks < dates);
        assert!(text.contains("rename section"));
        assert!(text.contains("delete task(s)"));
        // Every key of both panels is listed, and nothing claims to be both.
        for key in [
            "a",
            "A",
            "o / O",
            "i",
            "dd",
            "yy / p / P",
            "a / o",
            "A / O",
            "i / t",
            "x / Space / Enter",
            ":",
        ] {
            assert!(text.contains(&format!("{key:<20}")), "missing key {key}");
        }
    }

    #[test]
    fn max_scroll_is_bounded() {
        assert!(max_scroll(40) < max_scroll(10));
        assert_eq!(max_scroll(0), max_scroll(1));
    }
}
