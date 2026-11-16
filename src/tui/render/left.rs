//! Section tree panel.

use crate::task::{TodoList, get_node};
use crate::tui::state::{AppState, Focus, Mode, TextField, TreeItem};
use crate::tui::theme::Theme;
use crate::tui::widgets::{
    border_scrollbar, display_width, highlight, list_offset, split_at_char, truncate_ellipsis,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

/// Draw the section tree.
pub fn draw_left(f: &mut Frame, todo_list: &TodoList, app: &AppState, theme: &Theme, area: Rect) {
    let focused = app.focus == Focus::Left
        && !matches!(
            app.mode,
            Mode::Help | Mode::Conflict | Mode::Filter | Mode::InputTask(_) | Mode::InputDue(_)
        );

    let title = if app.filter.is_empty() {
        " Sections "
    } else {
        " Sections · filtered "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(title, theme.title(focused)))
        .border_style(theme.border(focused));

    let (renaming_node, rename_field, ghost_field) = match &app.mode {
        Mode::InputSection(input) => (input.node.as_ref(), Some(&input.field), Some(&input.field)),
        _ => (None, None, None),
    };

    let inner_width = usize::from(area.width.saturating_sub(2));
    let items: Vec<ListItem> = app
        .tree_items
        .iter()
        .map(|item| match item {
            TreeItem::Ghost(depth) => {
                let field = ghost_field.cloned().unwrap_or_default();
                ghost_item(&field, *depth, inner_width, theme)
            }
            TreeItem::Node(path) => {
                let Some(section) = get_node(todo_list, path) else {
                    return ListItem::new(Line::from(""));
                };
                let depth = path.len().saturating_sub(1);
                let has_children = !section.children.is_empty();
                let folded = app.folded.contains(path);
                let marker = if has_children {
                    if folded {
                        theme.glyphs.fold_closed
                    } else {
                        theme.glyphs.fold_open
                    }
                } else {
                    " "
                };

                let renaming = renaming_node.is_some_and(|node| node == path);
                let name_style = if depth == 0 {
                    theme
                        .text_style()
                        .add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    theme.text_style()
                };

                let total = section.count_tasks();
                let done = section.count_done_tasks();
                let progress = if total == 0 {
                    String::new()
                } else {
                    format!("{done}/{total}")
                };
                let prefix_width = depth * 2 + 2;
                let progress_width = display_width(&progress);
                let gap = usize::from(progress_width > 0);
                let name_room = inner_width.saturating_sub(prefix_width + progress_width + gap);
                let display_name = if renaming {
                    String::new()
                } else {
                    truncate_ellipsis(&section.name, name_room, theme.glyphs.ellipsis)
                };
                let name_width = if renaming {
                    rename_field.map_or(0, |field| field.buf.chars().count() + 1)
                } else {
                    display_width(&display_name)
                };

                let mut spans = vec![
                    Span::styled("  ".repeat(depth), theme.dim_style()),
                    Span::styled(format!("{marker} "), theme.fold_style()),
                ];
                if renaming {
                    if let Some(field) = rename_field {
                        spans.extend(editing_spans(field, name_style, theme));
                    }
                } else {
                    spans.extend(highlight(
                        &display_name,
                        &app.filter_lower,
                        name_style,
                        theme.match_style(),
                    ));
                }

                let left_width = prefix_width + name_width;
                let padding = inner_width
                    .saturating_sub(left_width)
                    .saturating_sub(progress_width);
                spans.push(Span::raw(" ".repeat(padding)));
                spans.push(Span::styled(progress, theme.meta_style()));
                ListItem::new(Line::from(spans))
            }
        })
        .collect();

    let height = usize::from(area.height.saturating_sub(2));
    let mut state = app.left_state;
    let offset = if let Some(selected) = state.selected() {
        let offset = list_offset(items.len(), selected, height);
        state = state.with_offset(offset);
        offset
    } else {
        0
    };
    let row_count = items.len();
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selection_style())
        .highlight_symbol("");
    f.render_stateful_widget(list, area, &mut state);
    draw_scrollbar(f, area, row_count, height, offset, theme);
}

fn ghost_item(field: &TextField, depth: usize, width: usize, theme: &Theme) -> ListItem<'static> {
    let mut spans = vec![
        Span::styled("  ".repeat(depth), theme.dim_style()),
        Span::raw("  "),
    ];
    spans.extend(editing_spans(field, theme.text_style(), theme));
    let used = depth * 2 + 2 + field.buf.chars().count() + 1;
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    ListItem::new(Line::from(spans))
}

fn editing_spans(field: &TextField, base: Style, theme: &Theme) -> Vec<Span<'static>> {
    let (before, after) = split_at_char(&field.buf, field.cursor);
    vec![
        Span::styled(before.to_string(), base),
        Span::styled(theme.glyphs.cursor.to_string(), theme.cursor_style()),
        Span::styled(after.to_string(), base),
    ]
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

    #[test]
    fn editing_spans_mark_the_cursor_position() {
        let field = TextField::new("abc");
        let mut field = field;
        field.cursor = 1;
        let spans = editing_spans(&field, Style::default(), &Theme::plain());
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "a");
        assert_eq!(spans[1].content, theme_cursor());
        assert_eq!(spans[2].content, "bc");
    }

    fn theme_cursor() -> &'static str {
        Theme::plain().glyphs.cursor
    }
}
