//! Bottom status bar: mode pill, transient message, and document totals.

use crate::task::TodoList;
use crate::tui::state::{AppState, Mode, TreeItem};
use crate::tui::task_ref::get_task_refs_filtered;
use crate::tui::theme::{ModePill, Theme};
use crate::tui::widgets::{display_width, truncate};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::collections::HashSet;
use std::path::Path;

/// Draw the status bar.
pub fn draw(
    f: &mut Frame,
    todo_list: &TodoList,
    app: &AppState,
    path: &Path,
    area: Rect,
    theme: &Theme,
) {
    let (label, pill) = match &app.mode {
        Mode::Normal => ("NORMAL", ModePill::Normal),
        Mode::Help => ("HELP", ModePill::Help),
        Mode::Filter => ("FILTER", ModePill::Filter),
        Mode::Conflict => ("CONFLICT", ModePill::Conflict),
        Mode::InputTask(_) | Mode::InputDue(_) | Mode::InputSection(_) => ("EDIT", ModePill::Edit),
    };

    let pill_text = format!(" {label} ");
    let (middle, middle_style) = middle_text(todo_list, app, theme);
    let file = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let right_text = format!(
        "{}/{}  {} ",
        todo_list.count_done_tasks(),
        todo_list.count_tasks(),
        file
    );

    let width = usize::from(area.width);
    let used = display_width(&pill_text) + display_width(&right_text);
    let available = width.saturating_sub(used).saturating_sub(1);
    let middle = truncate(&middle, available);
    let middle_width = display_width(&middle);
    let padding = available.saturating_sub(middle_width);

    let line = Line::from(vec![
        Span::styled(pill_text, theme.mode_pill(pill)),
        Span::raw(" "),
        Span::styled(middle, middle_style),
        Span::raw(" ".repeat(padding)),
        Span::styled(right_text, theme.meta_style()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn middle_text(todo_list: &TodoList, app: &AppState, theme: &Theme) -> (String, Style) {
    if matches!(app.mode, Mode::Filter) {
        let count = filter_match_count(todo_list, app);
        let matches = if count == 1 {
            "1 match".to_string()
        } else {
            format!("{count} matches")
        };
        if app.filter.is_empty() {
            return (format!("/  {matches} — type to filter"), theme.text_style());
        }
        return (format!("/{}  {matches}", app.filter), theme.text_style());
    }

    if let Some(message) = &app.message {
        return (message.text.clone(), theme.message_style(message.kind));
    }

    if matches!(app.mode, Mode::Help) {
        return (
            format!("j/k scroll {} Esc or q to close", theme.glyphs.separator),
            theme.dim_style(),
        );
    }

    let orphans = todo_list.unsectioned_tasks.len();
    if orphans > 0 {
        return (
            format!("{orphans} orphan task(s) before the first heading — not editable here"),
            theme.warning_style(),
        );
    }

    if !app.filter.is_empty() {
        let count = filter_match_count(todo_list, app);
        return (
            format!("filter: \"{}\"  ({count} visible)", app.filter),
            theme.dim_style(),
        );
    }

    (String::new(), theme.dim_style())
}

fn filter_match_count(todo_list: &TodoList, app: &AppState) -> usize {
    if app.filter_lower.is_empty() {
        return 0;
    }
    let mut matches = HashSet::new();
    for item in &app.tree_items {
        if let TreeItem::Node(path) = item {
            for task_ref in
                get_task_refs_filtered(todo_list, path, &app.filter_lower, app.hide_done)
            {
                matches.insert((task_ref.node.clone(), task_ref.task_idx));
            }
        }
    }
    matches.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_str;
    use crate::tui::state::AppState;
    use crate::tui::tree::build_visible_tree_items;

    #[test]
    fn filter_count_deduplicates_subtree_rows() {
        let todo_list = parse_str("# main\n- [ ] alpha\n## sub\n- [ ] alpha two\n");
        let mut app = AppState::new(1);
        app.set_filter("alpha".into());
        app.tree_items = build_visible_tree_items(
            &todo_list,
            &app.mode,
            &app.filter_lower,
            &app.folded,
            app.hide_done,
        );
        assert_eq!(filter_match_count(&todo_list, &app), 2);
    }

    #[test]
    fn orphan_warning_is_visible() {
        let todo_list = parse_str("- [ ] orphan\n# main\n");
        let app = AppState::new(1);
        let (text, _) = middle_text(&todo_list, &app, &Theme::plain());
        assert!(text.contains("1 orphan task"));
    }
}
