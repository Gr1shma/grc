//! Section tree panel.

use crate::task::{TodoList, child_count, get_node};
use crate::tui::render::Panel;
use crate::tui::state::{AppState, Mode, TextField, TreeItem};
use crate::tui::theme::{RowTheme, Theme};
use crate::tui::widgets::{
    GUTTER, border_scrollbar, display_width, highlight, list_offset, panel, split_at_char,
    truncate_ellipsis,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Scrollbar, ScrollbarOrientation, ScrollbarState};

/// Cells a section name must keep before the tree drops its `done/total`
/// counter, so a narrow panel never renders a nameless row.
const MIN_NAME_WIDTH: usize = 4;

/// Draw the section tree.
pub fn draw_left(f: &mut Frame, todo_list: &TodoList, app: &AppState, theme: &Theme, area: Rect) {
    let focused = Panel::Left.is_focused(app);

    let title = if app.filter.is_empty() {
        "Sections".to_string()
    } else {
        format!("Sections {} filtered", theme.glyphs.separator)
    };
    let block = panel(theme, &title, focused);

    let (renaming_node, rename_field, ghost_field) = match &app.mode {
        Mode::InputSection(input) => (input.node.as_ref(), Some(&input.field), Some(&input.field)),
        _ => (None, None, None),
    };

    let inner_width = usize::from(area.width.saturating_sub(2));
    let items: Vec<ListItem> = app
        .tree_items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let row = RowTheme::new(theme, app.left_state.selected() == Some(index), focused);
            match item {
                TreeItem::Ghost(depth) => {
                    let field = ghost_field.cloned().unwrap_or_default();
                    ghost_item(&field, *depth, inner_width, &row)
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
                            row.glyphs().fold_closed
                        } else {
                            row.glyphs().fold_open
                        }
                    } else {
                        " "
                    };

                    let renaming = renaming_node.is_some_and(|node| node == path);
                    let name_style = row.section_name(depth);

                    let total = section.count_tasks();
                    let done = section.count_done_tasks();
                    let prefix_width = depth * 2 + GUTTER + 2;
                    let counter = if total == 0 {
                        String::new()
                    } else {
                        format!("{done}/{total}")
                    };
                    // The name is what identifies the row, so a narrow panel
                    // drops the counter rather than truncating the name away.
                    let progress = if prefix_width + display_width(&counter) + 1 + MIN_NAME_WIDTH
                        > inner_width
                    {
                        String::new()
                    } else {
                        counter
                    };
                    let progress_width = display_width(&progress);
                    let gap = usize::from(progress_width > 0);
                    let name_room = inner_width.saturating_sub(prefix_width + progress_width + gap);
                    let display_name = if renaming {
                        String::new()
                    } else {
                        truncate_ellipsis(&section.name, name_room, row.glyphs().ellipsis)
                    };
                    let name_width = if renaming {
                        rename_field.map_or(0, |field| display_width(&field.buf) + 1)
                    } else {
                        display_width(&display_name)
                    };

                    let mut spans = vec![row.gutter()];
                    spans.extend(guide_prefix(todo_list, path, &row));
                    spans.push(Span::styled(format!("{marker} "), row.fold()));
                    if renaming {
                        if let Some(field) = rename_field {
                            spans.extend(editing_spans(field, name_style, &row));
                        }
                    } else {
                        spans.extend(highlight(
                            &display_name,
                            &app.filter_lower,
                            name_style,
                            row.matched(),
                        ));
                    }

                    let left_width = prefix_width + name_width;
                    let padding = inner_width
                        .saturating_sub(left_width)
                        .saturating_sub(progress_width);
                    spans.push(Span::raw(" ".repeat(padding)));
                    spans.push(Span::styled(progress, row.dim()));
                    ListItem::new(Line::from(spans))
                }
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
        .highlight_style(theme.selection_style(focused))
        .highlight_symbol("");
    f.render_stateful_widget(list, area, &mut state);
    draw_scrollbar(f, area, row_count, height, offset, theme);
}

/// Indentation for a section row, excluding the leading gutter.
///
/// Every level of depth contributes two cells. A level whose ancestor still has
/// later siblings draws a vertical guide, so a branch stays visually connected
/// to its parent; the last child of an ancestor leaves the cells blank.
fn guide_prefix(todo_list: &TodoList, path: &[usize], row: &RowTheme<'_>) -> Vec<Span<'static>> {
    let depth = path.len().saturating_sub(1);
    (0..depth)
        .map(|level| {
            let parent = (level > 0).then(|| path[..level].to_vec());
            let last = path[level] + 1 >= child_count(todo_list, parent.as_ref());
            let glyph = if last { " " } else { row.glyphs().tree_guide };
            Span::styled(format!("{glyph} "), row.guide())
        })
        .collect()
}

fn ghost_item(
    field: &TextField,
    depth: usize,
    width: usize,
    row: &RowTheme<'_>,
) -> ListItem<'static> {
    let indent = depth * 2 + GUTTER + 2;
    let mut spans = vec![
        row.gutter(),
        Span::raw(" ".repeat(indent.saturating_sub(GUTTER))),
    ];
    spans.extend(editing_spans(field, row.text(), row));
    let used = indent + field.buf.chars().count() + 1;
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    ListItem::new(Line::from(spans))
}

fn editing_spans(field: &TextField, base: Style, row: &RowTheme<'_>) -> Vec<Span<'static>> {
    let (before, after) = split_at_char(&field.buf, field.cursor);
    vec![
        Span::styled(before.to_string(), base),
        Span::styled(row.glyphs().cursor.to_string(), row.cursor()),
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
        let theme = Theme::plain();
        let row = RowTheme::new(&theme, false, true);
        let spans = editing_spans(&field, row.text(), &row);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "a");
        assert_eq!(spans[1].content, theme.glyphs.cursor);
        assert_eq!(spans[2].content, "bc");
    }

    #[test]
    fn the_gutter_holds_the_marker_only_for_the_selected_row() {
        let theme = Theme::plain();
        let field = TextField::new("x");
        let mark = |selected| {
            let row = RowTheme::new(&theme, selected, true);
            let item = ghost_item(&field, 2, 40, &row);
            item.width()
        };
        assert_eq!(mark(false), mark(true));
        let selected = RowTheme::new(&theme, true, true);
        assert_eq!(selected.gutter().content, theme.glyphs.selection_marker);
        assert_eq!(RowTheme::new(&theme, false, true).gutter().content, " ");
    }
}
