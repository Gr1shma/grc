//! Task panel.

use crate::due::{parse_due_token_from, resolve_relative_date_from};
use crate::task::{Task, TodoList, get_node};
use crate::tui::render::Panel;
use crate::tui::state::{AppState, Mode, TextField};
use crate::tui::task_ref::{TaskRef, full_breadcrumb, get_task_from_ref, get_task_refs_filtered};
use crate::tui::theme::{DueUrgency, RowTheme, Theme};
use crate::tui::tree::selected_node;
use crate::tui::widgets::{
    GUTTER, border_scrollbar, display_width, highlight, list_offset, panel, split_at_char,
    truncate_ellipsis,
};
use chrono::{Datelike, NaiveDate};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

/// Cells a task's own text must keep before the row gives up its breadcrumb.
///
/// The text identifies the row, so on a narrow panel the "section ›" prefix is
/// dropped first and only the text is allowed to end in an ellipsis.
const MIN_TEXT_WIDTH: usize = 4;

/// Draw the task list for the selected section.
pub fn draw_right(f: &mut Frame, todo_list: &TodoList, app: &AppState, theme: &Theme, area: Rect) {
    let focused = Panel::Right.is_focused(app);
    let title = selected_node(app)
        .map(|path| full_breadcrumb(todo_list, &path))
        .unwrap_or_else(|| "Tasks".to_string());
    let block = panel(theme, &title, focused);

    let task_refs = selected_node(app)
        .map(|node| get_task_refs_filtered(todo_list, &node, &app.filter_lower, app.hide_done))
        .unwrap_or_default();
    let width = usize::from(area.width.saturating_sub(2 + GUTTER as u16));
    let ctx = RowContext {
        app,
        today: app.today,
        width,
    };

    let (editing_idx, editing_field) = match &app.mode {
        Mode::InputTask(input) => (input.editing_idx, Some(&input.field)),
        _ => (None, None),
    };
    let due_target = match &app.mode {
        Mode::InputDue(input) => Some(&input.target),
        _ => None,
    };
    let due_field = match &app.mode {
        Mode::InputDue(input) => Some(&input.field),
        _ => None,
    };

    // The insertion ghost is spliced into the list below, so it takes a row
    // index of its own and shifts every row after it by one.
    let ghost_position = match &app.mode {
        Mode::InputTask(input) if input.editing_idx.is_none() => Some(
            input
                .insert_idx
                .unwrap_or(task_refs.len())
                .min(task_refs.len()),
        ),
        _ => None,
    };
    let selected = app.right_state.selected();
    let selected_ref = selected.and_then(|selected| match ghost_position {
        Some(ghost) if selected == ghost => None,
        Some(ghost) if selected > ghost => Some(selected - 1),
        _ => Some(selected),
    });

    let mut items: Vec<ListItem> = task_refs
        .iter()
        .enumerate()
        .map(|(index, task_ref)| {
            let row = RowTheme::new(theme, selected_ref == Some(index), focused);
            let Some(task) = get_task_from_ref(todo_list, task_ref) else {
                return ListItem::new(Line::from(""));
            };
            if Some(index) == editing_idx {
                let field = editing_field.expect("edit field exists for editing row");
                editing_item(
                    &ctx,
                    field,
                    task.is_done,
                    task_ref.sub_name.as_deref(),
                    &row,
                )
            } else if due_target == Some(task_ref) {
                let field = due_field.expect("due field exists for due row");
                due_editing_item(&ctx, task, field, task_ref.sub_name.as_deref(), &row)
            } else {
                task_item(&ctx, task, task_ref, &row)
            }
        })
        .collect();

    if let (Mode::InputTask(input), Some(position)) = (&app.mode, ghost_position) {
        let row = RowTheme::new(theme, selected == Some(position), focused);
        items.insert(position, ghost_item(&ctx, &input.field, &row));
    }

    if items.is_empty() {
        f.render_widget(empty_state(todo_list, app, block, theme, width), area);
        return;
    }

    let height = usize::from(area.height.saturating_sub(2));
    let row_count = items.len();
    let mut state = app.right_state;
    let offset = if let Some(selected) = state.selected() {
        let offset = list_offset(items.len(), selected, height);
        state = state.with_offset(offset);
        offset
    } else {
        0
    };
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selection_style(focused))
        .highlight_symbol("");
    f.render_stateful_widget(list, area, &mut state);
    draw_scrollbar(f, area, row_count, height, offset, theme);
}

struct RowContext<'a> {
    app: &'a AppState,
    today: NaiveDate,
    width: usize,
}

/// Cells between the checkbox and the task text, once the breadcrumb that did
/// not fit has been dropped.
fn text_room(ctx: &RowContext<'_>, used: usize, breadcrumb: Option<usize>, due: usize) -> usize {
    ctx.width
        .saturating_sub(used + breadcrumb.unwrap_or(0) + due + 1)
}

fn task_item(
    ctx: &RowContext<'_>,
    task: &Task,
    task_ref: &TaskRef,
    row: &RowTheme<'_>,
) -> ListItem<'static> {
    let (glyph, glyph_style) = row.checkbox(task.is_done);
    let mut spans = vec![
        row.gutter(),
        Span::styled(glyph.to_string(), glyph_style),
        Span::raw(" "),
    ];
    let mut used = GUTTER + display_width(glyph) + 1;

    let due = task.due.map(|date| {
        (
            format_due(date, ctx.today),
            row.due(due_urgency(task.is_done, date, ctx.today)),
        )
    });
    let due_width = due.as_ref().map_or(0, |(label, _)| display_width(label));

    let mut breadcrumb = task_ref
        .sub_name
        .as_ref()
        .map(|name| format!("{name} {} ", row.glyphs().breadcrumb_sep));
    let breadcrumb_width = breadcrumb.as_ref().map(|text| display_width(text));
    if text_room(ctx, used, breadcrumb_width, due_width) < MIN_TEXT_WIDTH {
        breadcrumb = None;
    }
    if let Some(breadcrumb) = breadcrumb {
        used += display_width(&breadcrumb);
        spans.push(Span::styled(breadcrumb, row.breadcrumb()));
    }

    let room = text_room(ctx, used, None, due_width);
    let text = truncate_ellipsis(&task.text, room, row.glyphs().ellipsis);
    let text_style = if task.is_done { row.done() } else { row.text() };
    spans.extend(highlight(
        &text,
        &ctx.app.filter_lower,
        text_style,
        row.matched(),
    ));
    used += display_width(&text);

    spans.push(Span::raw(
        " ".repeat(ctx.width.saturating_sub(used + due_width)),
    ));
    if let Some((label, style)) = due {
        spans.push(Span::styled(label, style));
    }
    ListItem::new(Line::from(spans))
}

fn editing_item(
    ctx: &RowContext<'_>,
    field: &TextField,
    is_done: bool,
    sub_name: Option<&str>,
    row: &RowTheme<'_>,
) -> ListItem<'static> {
    let (glyph, glyph_style) = row.checkbox(is_done);
    let mut spans = vec![
        row.gutter(),
        Span::styled(glyph.to_string(), glyph_style),
        Span::raw(" "),
    ];
    if let Some(name) = sub_name {
        spans.push(Span::styled(
            format!("{name} {} ", row.glyphs().breadcrumb_sep),
            row.breadcrumb(),
        ));
    }
    let due = inline_due(&field.buf, ctx.today);
    spans.extend(buffer_spans(field, due.as_ref(), row.text(), row));
    if let Some(preview) = due_preview(due.as_ref(), ctx.today, row) {
        spans.push(preview);
    }
    ListItem::new(Line::from(spans))
}

fn ghost_item(ctx: &RowContext<'_>, field: &TextField, row: &RowTheme<'_>) -> ListItem<'static> {
    let (glyph, glyph_style) = row.checkbox(false);
    let mut spans = vec![
        row.gutter(),
        Span::styled(glyph.to_string(), glyph_style),
        Span::raw(" "),
    ];
    let due = inline_due(&field.buf, ctx.today);
    spans.extend(buffer_spans(field, due.as_ref(), row.text(), row));
    if let Some(preview) = due_preview(due.as_ref(), ctx.today, row) {
        spans.push(preview);
    }
    ListItem::new(Line::from(spans))
}

fn due_editing_item(
    ctx: &RowContext<'_>,
    task: &Task,
    field: &TextField,
    sub_name: Option<&str>,
    row: &RowTheme<'_>,
) -> ListItem<'static> {
    let (glyph, glyph_style) = row.checkbox(task.is_done);
    let mut spans = vec![
        row.gutter(),
        Span::styled(glyph.to_string(), glyph_style),
        Span::raw(" "),
    ];
    let mut used = GUTTER + display_width(glyph) + 1;

    let prefix = "due:";
    let parsed = resolve_relative_date_from(field.buf.trim(), ctx.today);
    let field_style = match parsed {
        Some(_) => row.matched(),
        None if field.buf.trim().is_empty() => row.dim(),
        None => row.error(),
    };
    let (preview_text, preview_style) = due_preview_text(
        parsed,
        field.buf.trim().is_empty(),
        task.due.is_some(),
        ctx.today,
        row,
    );
    let suffix_width =
        display_width(prefix) + field.buf.chars().count() + 1 + display_width(&preview_text);

    let mut breadcrumb = sub_name.map(|name| format!("{name} {} ", row.glyphs().breadcrumb_sep));
    let breadcrumb_width = breadcrumb.as_ref().map(|text| display_width(text));
    if text_room(ctx, used, breadcrumb_width, suffix_width) < MIN_TEXT_WIDTH {
        breadcrumb = None;
    }
    if let Some(breadcrumb) = breadcrumb {
        used += display_width(&breadcrumb);
        spans.push(Span::styled(breadcrumb, row.breadcrumb()));
    }

    let room = text_room(ctx, used, None, suffix_width);
    let text = truncate_ellipsis(&task.text, room, row.glyphs().ellipsis);
    spans.push(Span::styled(text.clone(), row.text()));
    used += display_width(&text);
    spans.push(Span::raw(
        " ".repeat(ctx.width.saturating_sub(used + suffix_width)),
    ));
    spans.push(Span::styled(prefix.to_string(), row.dim()));
    spans.extend(buffer_spans(field, None, field_style, row));
    spans.push(Span::styled(preview_text, preview_style));
    ListItem::new(Line::from(spans))
}

fn empty_state<'a>(
    todo_list: &TodoList,
    app: &AppState,
    block: Block<'a>,
    theme: &Theme,
    width: usize,
) -> Paragraph<'a> {
    let narrow = width < 34;
    let text = match selected_node(app) {
        None => {
            if narrow {
                "select a section — h"
            } else {
                "select a section — press h"
            }
        }
        Some(_path) if !app.filter.is_empty() => {
            if narrow {
                "no matches — Esc clears"
            } else {
                "no matches — press Esc to clear"
            }
        }
        Some(path) => {
            let section = get_node(todo_list, &path);
            let hidden_done = app.hide_done
                && section.is_some_and(|section| section.count_done_tasks() > 0)
                && section.is_some_and(|section| section.count_tasks() > 0);
            if hidden_done {
                if narrow {
                    "all done — H shows"
                } else {
                    "all tasks done — press H to show"
                }
            } else if narrow {
                "no tasks — o adds"
            } else {
                "no tasks — press o to add one"
            }
        }
    };
    let style = theme.dim_style();
    let hint = Line::from(vec![
        Span::raw(" ".repeat(GUTTER + 1)),
        Span::styled(text.to_string(), style),
    ]);
    Paragraph::new(hint).block(block)
}

fn due_urgency(is_done: bool, date: NaiveDate, today: NaiveDate) -> DueUrgency {
    if is_done {
        DueUrgency::Done
    } else {
        urgency_for_date(date, today)
    }
}

fn urgency_for_date(date: NaiveDate, today: NaiveDate) -> DueUrgency {
    let delta = (date - today).num_days();
    if delta < 0 {
        DueUrgency::Overdue
    } else if delta == 0 {
        DueUrgency::Today
    } else if delta <= 3 {
        DueUrgency::Soon
    } else {
        DueUrgency::Later
    }
}

/// Human due-date label. ISO dates never appear in the task list.
pub(crate) fn format_due(date: NaiveDate, today: NaiveDate) -> String {
    let delta = (date - today).num_days();
    match delta {
        0 => "today".to_string(),
        1 => "tmr".to_string(),
        2..=6 => weekday_short(date).to_string(),
        -1 => "yday".to_string(),
        -6..=-2 => format!("{}d ago", -delta),
        _ if date.year() == today.year() => {
            format!("{} {}", month_short(date.month()), date.day())
        }
        _ => format!(
            "{} {}, {}",
            month_short(date.month()),
            date.day(),
            date.year()
        ),
    }
}

fn weekday_short(date: NaiveDate) -> &'static str {
    match date.weekday() {
        chrono::Weekday::Mon => "mon",
        chrono::Weekday::Tue => "tue",
        chrono::Weekday::Wed => "wed",
        chrono::Weekday::Thu => "thu",
        chrono::Weekday::Fri => "fri",
        chrono::Weekday::Sat => "sat",
        chrono::Weekday::Sun => "sun",
    }
}

fn month_short(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

struct InlineDue {
    start: usize,
    end: usize,
    parsed: Option<NaiveDate>,
}

fn inline_due(buf: &str, today: NaiveDate) -> Option<InlineDue> {
    let candidates: Vec<usize> = buf
        .match_indices("due:")
        .filter(|(index, _)| {
            *index == 0
                || buf[..*index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace)
        })
        .map(|(index, _)| index)
        .collect();

    // Match `extract_inline_due_from`: the last token that parses is the one
    // that will be applied, so a trailing typo must not hide it.
    for index in candidates.iter().rev() {
        let after = &buf[*index + "due:".len()..];
        if let Some((date, consumed)) = parse_due_token_from(after, today) {
            return Some(InlineDue {
                start: *index,
                end: *index + "due:".len() + consumed,
                parsed: Some(date),
            });
        }
    }

    let index = *candidates.last()?;
    let after = &buf[index + "due:".len()..];
    let end = after
        .find(char::is_whitespace)
        .map_or(buf.len(), |offset| index + "due:".len() + offset);
    Some(InlineDue {
        start: index,
        end,
        parsed: None,
    })
}

fn buffer_spans(
    field: &TextField,
    due: Option<&InlineDue>,
    base: Style,
    row: &RowTheme<'_>,
) -> Vec<Span<'static>> {
    let buf = field.buf.as_str();
    let cursor = split_at_char(buf, field.cursor).0.len();
    let mut boundaries = vec![0, buf.len(), cursor];
    if let Some(due) = due {
        boundaries.push(due.start.min(buf.len()));
        boundaries.push(due.end.min(buf.len()));
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut spans = Vec::new();
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start == cursor {
            spans.push(cursor_span(row));
        }
        let style = match due {
            Some(due) if start >= due.start && end <= due.end => {
                if due.parsed.is_some() {
                    row.matched()
                } else {
                    row.error()
                }
            }
            _ => base,
        };
        if end > start {
            spans.push(Span::styled(buf[start..end].to_string(), style));
        }
    }
    if cursor == buf.len() {
        spans.push(cursor_span(row));
    }
    spans
}

fn cursor_span(row: &RowTheme<'_>) -> Span<'static> {
    Span::styled(row.glyphs().cursor.to_string(), row.cursor())
}

fn due_preview_text(
    parsed: Option<NaiveDate>,
    empty: bool,
    had_date: bool,
    today: NaiveDate,
    row: &RowTheme<'_>,
) -> (String, Style) {
    match parsed {
        Some(date) => (
            format!("  {} {}", row.glyphs().preview, format_due(date, today)),
            row.due(urgency_for_date(date, today)),
        ),
        None if empty => (format!("  {} cleared", row.glyphs().preview), row.dim()),
        // "old date kept" is only true when this task already had one.
        None if had_date => (
            format!("  {} invalid — old date kept", row.glyphs().invalid_due),
            row.error(),
        ),
        None => (
            format!("  {} invalid date", row.glyphs().invalid_due),
            row.error(),
        ),
    }
}

fn due_preview(
    due: Option<&InlineDue>,
    today: NaiveDate,
    row: &RowTheme<'_>,
) -> Option<Span<'static>> {
    let due = due?;
    Some(match due.parsed {
        Some(date) => Span::styled(
            format!("  {} {}", row.glyphs().preview, format_due(date, today)),
            row.due(urgency_for_date(date, today)),
        ),
        None => Span::styled(
            format!("  {} invalid due", row.glyphs().invalid_due),
            row.error(),
        ),
    })
}

fn draw_scrollbar(
    f: &mut Frame,
    area: Rect,
    total: usize,
    height: usize,
    offset: usize,
    theme: &Theme,
) {
    let strip = border_scrollbar(area);
    if height == 0 || total <= height || strip.width == 0 {
        return;
    }
    let mut state = ScrollbarState::new(total)
        .viewport_content_length(height)
        .position(offset);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some(theme.glyphs.scroll_track))
        .thumb_symbol(theme.glyphs.scroll_thumb)
        .track_style(theme.scrollbar_track())
        .thumb_style(theme.scrollbar_thumb());
    f.render_stateful_widget(scrollbar, strip, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 7, 21).unwrap()
    }

    #[test]
    fn due_labels_are_human_and_stable() {
        assert_eq!(format_due(today(), today()), "today");
        assert_eq!(
            format_due(NaiveDate::from_ymd_opt(2025, 7, 22).unwrap(), today()),
            "tmr"
        );
        assert_eq!(
            format_due(NaiveDate::from_ymd_opt(2025, 7, 23).unwrap(), today()),
            "wed"
        );
        assert_eq!(
            format_due(NaiveDate::from_ymd_opt(2025, 9, 15).unwrap(), today()),
            "Sep 15"
        );
        assert_eq!(
            format_due(NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(), today()),
            "Jan 2, 2026"
        );
        assert_eq!(
            format_due(NaiveDate::from_ymd_opt(2025, 7, 20).unwrap(), today()),
            "yday"
        );
        assert_eq!(
            format_due(NaiveDate::from_ymd_opt(2025, 7, 18).unwrap(), today()),
            "3d ago"
        );
    }

    #[test]
    fn due_urgency_uses_overdue_today_soon_later() {
        assert_eq!(
            urgency_for_date(NaiveDate::from_ymd_opt(2025, 7, 20).unwrap(), today()),
            DueUrgency::Overdue
        );
        assert_eq!(urgency_for_date(today(), today()), DueUrgency::Today);
        assert_eq!(
            urgency_for_date(NaiveDate::from_ymd_opt(2025, 7, 24).unwrap(), today()),
            DueUrgency::Soon
        );
        assert_eq!(
            urgency_for_date(NaiveDate::from_ymd_opt(2025, 8, 1).unwrap(), today()),
            DueUrgency::Later
        );
    }

    #[test]
    fn due_preview_only_mentions_a_kept_date_when_there_was_one() {
        let theme = Theme::plain();
        let row = RowTheme::new(&theme, false, true);
        let (kept, _) = due_preview_text(None, false, true, today(), &row);
        assert!(kept.contains("old date kept"));
        let (first, _) = due_preview_text(None, false, false, today(), &row);
        assert!(!first.contains("kept"));
        assert!(first.contains("invalid date"));

        let (cleared, _) = due_preview_text(None, true, true, today(), &row);
        assert!(cleared.contains("cleared"));
    }

    #[test]
    fn inline_due_finds_preview_ranges() {
        let due = inline_due("Ship it due:next mon", today()).unwrap();
        assert_eq!(&"Ship it due:next mon"[due.start..due.end], "due:next mon");
        assert_eq!(
            due.parsed,
            Some(NaiveDate::from_ymd_opt(2025, 7, 28).unwrap())
        );

        let invalid = inline_due("Ship it due:nonsense", today()).unwrap();
        assert_eq!(
            &"Ship it due:nonsense"[invalid.start..invalid.end],
            "due:nonsense"
        );
        assert!(invalid.parsed.is_none());
    }

    #[test]
    fn inline_due_previews_the_token_that_will_be_applied() {
        // Commit takes the last parseable token; the preview must agree even
        // when a later token is unparseable.
        let buf = "Ship it due:tomorrow due:nonsense";
        let due = inline_due(buf, today()).unwrap();
        assert_eq!(&buf[due.start..due.end], "due:tomorrow");
        assert_eq!(
            due.parsed,
            Some(NaiveDate::from_ymd_opt(2025, 7, 22).unwrap())
        );
        assert_eq!(
            crate::due::extract_inline_due_from(buf, today()),
            ("Ship it due:nonsense".to_string(), due.parsed)
        );
    }
}
