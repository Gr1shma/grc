pub mod input;
pub mod render;
pub mod section_ops;
pub mod state;
pub mod task_ref;
pub mod tree;

pub use section_ops::*;
pub use task_ref::*;
pub use tree::*;

use crate::parser::parse_file;
use crate::task::TodoList;
use crate::tui::input::{InputDueParams, InputSectionParams, InputTaskParams};
use crate::tui::state::{AppState, Focus, Mode, TreeItem};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyModifiers, MouseButton, MouseEvent},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use std::io::{self, stdout};
use std::path::Path;

/// Width of the left (sections) panel as a percentage of the terminal width.
/// Shared by the renderer and mouse hit-testing so they can never diverge.
pub const LEFT_PANEL_PERCENT: u16 = 27;

/// Help popup size: width percentage and row height. Shared by the help
/// renderer and the click-outside-to-close hit test.
pub const HELP_POPUP_WIDTH_PERCENT: u16 = 55;
pub const HELP_POPUP_HEIGHT: u16 = 34;

pub fn run_tui(path: &Path) -> Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).context("enter alternate screen")?;
    execute!(out, event::EnableMouseCapture).context("enable mouse capture")?;
    let backend = CrosstermBackend::new(out);
    let mut term = Terminal::new(backend).context("create terminal")?;

    // Guard struct ensures terminal cleanup even on panic
    struct TerminalGuard<'a> {
        term: &'a mut Terminal<CrosstermBackend<io::Stdout>>,
    }
    impl<'a> Drop for TerminalGuard<'a> {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(self.term.backend_mut(), LeaveAlternateScreen);
            let _ = self.term.show_cursor();
            let _ = execute!(self.term.backend_mut(), event::DisableMouseCapture);
        }
    }

    let guard = TerminalGuard { term: &mut term };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_engine(guard.term, path)
    }));

    drop(guard); // Cleanup runs here

    match result {
        Ok(inner_result) => inner_result,
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            Err(anyhow::anyhow!("TUI panicked: {}", msg))
        }
    }
}

fn run_engine(term: &mut Terminal<CrosstermBackend<io::Stdout>>, path: &Path) -> Result<()> {
    let mut todo_list = parse_file(path)?;
    let mut app = AppState::new(todo_list.sections.len());

    loop {
        app.tree_items = build_visible_tree_items(&todo_list, &app.mode, &app.filter_lower);
        if let Some(node) = app.filter_restore_node.take()
            && let Some(pos) = app
                .tree_items
                .iter()
                .position(|item| matches!(item, TreeItem::Node(p) if p == &node))
        {
            app.left_state.select(Some(pos));
            if let (Some(task_node), Some(task_idx)) = (
                app.filter_restore_task_node.take(),
                app.filter_restore_task_idx.take(),
            ) {
                let refs = get_task_refs_filtered(&todo_list, &node, "");
                if let Some(new_pos) = refs
                    .iter()
                    .position(|r| r.node == task_node && r.task_idx == task_idx)
                {
                    app.right_state.select(Some(new_pos));
                } else {
                    app.right_state.select(Some(0));
                }
            } else {
                app.right_state.select(Some(0));
            }
        }
        sync_selection_states(&mut app, &todo_list);
        term.draw(|f| render::draw_ui(f, &todo_list, &mut app))?;

        let (columns, rows) = size()?;
        let area = Rect::new(0, 0, columns, rows);

        match event::read()? {
            Event::Key(key) if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == event::KeyCode::Char('c') =>
            {
                break;
            }

            Event::Key(key) => {
                let quit = match &app.mode {
                    Mode::Normal => input::handle_normal(&mut app, &mut todo_list, path, key.code)?,
                    Mode::Help => {
                        if key.code == event::KeyCode::Esc
                            || key.code == event::KeyCode::Char('q')
                            || key.code == event::KeyCode::Char('?')
                        {
                            app.mode = Mode::Normal;
                        } else if key.code == event::KeyCode::Char('/') {
                            app.mode = Mode::Filter;
                            app.left_state.select(Some(0));
                            app.right_state.select(Some(0));
                        } else if key.code == event::KeyCode::Down || key.code == event::KeyCode::Char('j') {
                            app.help_scroll = app.help_scroll.saturating_add(1);
                        } else if key.code == event::KeyCode::Up || key.code == event::KeyCode::Char('k') {
                            app.help_scroll = app.help_scroll.saturating_sub(1);
                        }
                        false
                    }
                    Mode::InputTask {
                        editing_idx,
                        insert_idx,
                        above,
                        buf,
                        cursor,
                    } => {
                        let mut params = InputTaskParams {
                            editing_idx: *editing_idx,
                            insert_idx: *insert_idx,
                            above: *above,
                            buf: buf.clone(),
                            cursor: *cursor,
                        };
                        input::handle_input_task(
                            &mut app,
                            &mut todo_list,
                            path,
                            key.code,
                            key.modifiers,
                            &mut params,
                        )?;
                        false
                    }
                    Mode::InputDue {
                        task_idx,
                        buf,
                        cursor,
                    } => {
                        let mut params = InputDueParams {
                            task_idx: *task_idx,
                            buf: buf.clone(),
                            cursor: *cursor,
                        };
                        input::handle_input_due(&mut app, &mut todo_list, path, key.code, key.modifiers, &mut params)?;
                        false
                    }
                    Mode::InputSection {
                        node,
                        parent,
                        insert_idx,
                        buf,
                        cursor,
                    } => {
                        let mut params = InputSectionParams {
                            node: node.clone(),
                            parent: parent.clone(),
                            insert_idx: *insert_idx,
                            buf: buf.clone(),
                            cursor: *cursor,
                        };
                        input::handle_input_section(
                            &mut app,
                            &mut todo_list,
                            path,
                            key.code,
                            key.modifiers,
                            &mut params,
                        )?;
                        false
                    }
                    Mode::Filter => {
                        input::handle_filter(&mut app, &todo_list, key.code)?;
                        false
                    }
                };

                if quit {
                    break;
                }
            }

            Event::Mouse(evt) => handle_mouse(&mut app, &todo_list, area, evt),

            Event::Resize(_, _) => {}

            _ => {}
        }
    }
    Ok(())
}

fn handle_mouse(app: &mut AppState, todo_list: &TodoList, area: Rect, evt: MouseEvent) {
    use crossterm::event::MouseEventKind;

    if matches!(
        app.mode,
        Mode::InputTask { .. }
            | Mode::InputDue { .. }
            | Mode::InputSection { .. }
            | Mode::Filter
    ) {
        app.pending_d = false;
        app.pending_g = false;
        app.pending_y = false;
        return;
    }

    let MouseEvent { kind, column, row, .. } = evt;

    if app.mode == Mode::Help {
        if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
            let delta = if matches!(kind, MouseEventKind::ScrollUp) { -1i16 } else { 1i16 };
            handle_mouse_scroll(app, todo_list, delta);
            return;
        }
        if matches!(kind, MouseEventKind::Down(MouseButton::Left)) {
            let popup_width = area.width * HELP_POPUP_WIDTH_PERCENT / 100;
            let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
            let y = area.y + (area.height.saturating_sub(HELP_POPUP_HEIGHT)) / 2;
            let inside = column >= x
                && column < x + popup_width.min(area.width)
                && row >= y
                && row < y + HELP_POPUP_HEIGHT.min(area.height);
            if !inside {
                app.mode = Mode::Normal;
            }
        }
        return;
    }

    match kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let delta = if matches!(kind, MouseEventKind::ScrollUp) { -1i16 } else { 1i16 };
            handle_mouse_scroll(app, todo_list, delta);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // Ignore clicks on the top border row (index 0, no items) and the
            // bottom status-bar row. `row.checked_sub(1)` also guards the
            // degenerate `area.height == 0` case.
            let Some(idx) = row.checked_sub(1).map(|r| r as usize) else {
                return;
            };
            if idx + 2 >= area.height as usize {
                return;
            }

            let left_width = area.width * LEFT_PANEL_PERCENT / 100;

            if column < left_width {
                app.focus = Focus::Left;
                if idx < app.tree_items.len() {
                    app.left_state.select(Some(idx));
                }
            } else {
                app.focus = Focus::Right;
                if let Some(node) = selected_node(app) {
                    let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                    if idx < refs.len() {
                        app.right_state.select(Some(idx));
                        return;
                    }
                }
                if !app.tree_items.is_empty() {
                    app.left_state.select(Some(
                        app.left_state.selected().unwrap_or(0).min(app.tree_items.len() - 1)
                    ));
                }
            }
        }
        _ => {}
    }
}

fn handle_mouse_scroll(app: &mut AppState, todo_list: &TodoList, delta: i16) {
    if app.mode == Mode::Help {
        app.help_scroll = app.help_scroll.saturating_add_signed(delta);
        return;
    }
    match app.focus {
        Focus::Left => {
            let max = app.tree_items.len().saturating_sub(1);
            let cur = app.left_state.selected().unwrap_or(0) as i16;
            app.left_state.select(Some(
                cur.saturating_add(delta).min(max as i16).max(0) as usize
            ));
        }
        Focus::Right => {
            if let Some(node) = selected_node(app) {
                let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                let max = refs.len().saturating_sub(1);
                let cur = app.right_state.selected().unwrap_or(0) as i16;
                app.right_state.select(Some(
                    cur.saturating_add(delta).min(max as i16).max(0) as usize
                ));
            }
        }
    }
}

fn sync_selection_states(app: &mut AppState, todo_list: &TodoList) {
    match &app.mode {
        Mode::InputSection { node: None, .. } => {
            if let Some(pos) = app
                .tree_items
                .iter()
                .position(|i| matches!(i, TreeItem::Ghost(_)))
            {
                app.left_state.select(Some(pos));
            }
        }
        Mode::InputTask {
            editing_idx: None,
            insert_idx,
            ..
        } => {
            if let Some(node) = selected_node(app) {
                let task_refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                let pos = insert_idx.unwrap_or(task_refs.len()).min(task_refs.len());
                app.right_state.select(Some(pos));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Section, Task, TodoList};
    use crate::tui::state::{AppState, Mode, TreeItem};

    fn sample_todo_list() -> TodoList {
        TodoList {
            sections: vec![
                Section {
                    name: "work".to_string(),
                    tasks: vec![
                        Task {
                            text: "Task A".to_string(),
                            is_done: false,
                            due: None,
                        },
                        Task {
                            text: "Task B".to_string(),
                            is_done: true,
                            due: None,
                        },
                    ],
                    children: vec![Section {
                        name: "backend".to_string(),
                        tasks: vec![Task {
                            text: "Fix API".to_string(),
                            is_done: false,
                            due: None,
                        }],
                        children: vec![Section {
                            name: "api".to_string(),
                            tasks: vec![Task {
                                text: "Fix endpoint".to_string(),
                                is_done: false,
                                due: None,
                            }],
                            children: vec![],
                        }],
                    }],
                },
                Section {
                    name: "personal".to_string(),
                    tasks: vec![Task {
                        text: "Buy milk".to_string(),
                        is_done: false,
                        due: None,
                    }],
                    children: vec![],
                },
            ],
        }
    }

    #[test]
    fn build_tree_items_in_normal_mode() {
        let list = sample_todo_list();
        let items = build_tree_items(&list, &Mode::Normal);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], TreeItem::Node(vec![0]));
        assert_eq!(items[1], TreeItem::Node(vec![0, 0]));
        assert_eq!(items[2], TreeItem::Node(vec![0, 0, 0]));
        assert_eq!(items[3], TreeItem::Node(vec![1]));
    }

    #[test]
    fn build_tree_items_empty_list() {
        let list = TodoList::default();
        let items = build_tree_items(&list, &Mode::Normal);
        assert!(items.is_empty());
    }

    #[test]
    fn selected_node_returns_none_when_nothing_selected() {
        let mut app = AppState::new(0);
        app.tree_items = vec![TreeItem::Node(vec![0])];
        assert!(selected_node(&app).is_none());
    }

    #[test]
    fn selected_node_returns_correct_node() {
        let mut app = AppState::new(2);
        app.tree_items = vec![
            TreeItem::Node(vec![0]),
            TreeItem::Node(vec![0, 0]),
            TreeItem::Node(vec![1]),
        ];
        app.left_state.select(Some(1));
        assert_eq!(selected_node(&app), Some(vec![0, 0]));
    }

    #[test]
    fn selected_node_returns_none_on_ghost() {
        let mut app = AppState::new(0);
        app.tree_items = vec![TreeItem::Ghost(0)];
        app.left_state.select(Some(0));
        assert!(selected_node(&app).is_none());
    }

    #[test]
    fn node_name_for_section() {
        let list = sample_todo_list();
        assert_eq!(node_name(&list, &vec![0]), "work");
        assert_eq!(node_name(&list, &vec![1]), "personal");
        assert_eq!(node_name(&list, &vec![0, 0]), "backend");
    }

    #[test]
    fn build_visible_tree_items_empty_filter_keeps_all() {
        let list = sample_todo_list();
        let items = build_visible_tree_items(&list, &Mode::Normal, "");
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn build_visible_tree_items_filters_sections_case_insensitive() {
        let list = sample_todo_list();
        // Filter is pre-lowercased by AppState, so pass lowercase here
        let items = build_visible_tree_items(&list, &Mode::Normal, "personal");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0], TreeItem::Node(vec![1]));
    }

    #[test]
    fn build_visible_tree_items_includes_matching_parent_of_nested_task() {
        let list = sample_todo_list();
        // "endpoint" only appears in the nested "api" task; its ancestors must stay visible.
        let items = build_visible_tree_items(&list, &Mode::Normal, "endpoint");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], TreeItem::Node(vec![0]));
        assert_eq!(items[1], TreeItem::Node(vec![0, 0]));
        assert_eq!(items[2], TreeItem::Node(vec![0, 0, 0]));
    }

    #[test]
    fn get_task_refs_filtered_narrows_by_text() {
        let list = sample_todo_list();
        let refs = get_task_refs_filtered(&list, &vec![0], "backend");
        // Matches the "backend" subsection task directly and the nested "api" task via its
        // breadcrumb (sub_name "backend › api").
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].node, vec![0, 0]);
        assert_eq!(refs[0].task_idx, 0);
        assert_eq!(refs[1].node, vec![0, 0, 0]);
        assert_eq!(refs[1].task_idx, 0);
    }

    #[test]
    fn get_task_refs_filtered_empty_filter_returns_all() {
        let list = sample_todo_list();
        let refs = get_task_refs_filtered(&list, &vec![0], "");
        assert_eq!(refs.len(), 4);
    }

    #[test]
    fn get_task_refs_filtered_shows_all_tasks_when_top_level_section_matches() {
        let list = sample_todo_list();
        let refs = get_task_refs_filtered(&list, &vec![0], "work");
        assert_eq!(refs.len(), 4);
    }

    #[test]
    fn get_task_refs_filtered_shows_direct_tasks_when_nested_section_matches() {
        let list = sample_todo_list();
        let refs = get_task_refs_filtered(&list, &vec![0, 0], "backend");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].node, vec![0, 0]);
        assert_eq!(refs[0].task_idx, 0);
    }

    #[test]
    fn get_task_refs_filtered_still_filters_when_section_does_not_match() {
        let list = sample_todo_list();
        let refs = get_task_refs_filtered(&list, &vec![0], "milk");
        assert!(refs.is_empty());
    }

    #[test]
    fn count_tasks_includes_all_descendants() {
        let list = sample_todo_list();
        assert_eq!(list.sections[0].count_tasks(), 4);
    }

    #[test]
    fn count_tasks_section_only() {
        let list = sample_todo_list();
        assert_eq!(list.sections[1].count_tasks(), 1);
    }

    #[test]
    fn count_tasks_empty_section() {
        let sec = Section {
            name: "empty".to_string(),
            tasks: Vec::new(),
            children: Vec::new(),
        };
        assert_eq!(sec.count_tasks(), 0);
    }

    #[test]
    fn task_refs_for_top_level_flattens_subtree() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, &vec![0]);
        assert_eq!(refs.len(), 4);
        assert_eq!(refs[0].node, vec![0]);
        assert_eq!(refs[0].task_idx, 0);
        assert_eq!(refs[0].sub_name, None);
        assert_eq!(refs[1].node, vec![0]);
        assert_eq!(refs[1].task_idx, 1);
        assert_eq!(refs[1].sub_name, None);
        assert_eq!(refs[2].node, vec![0, 0]);
        assert_eq!(refs[2].task_idx, 0);
        assert_eq!(refs[2].sub_name.as_deref(), Some("backend"));
        assert_eq!(refs[3].node, vec![0, 0, 0]);
        assert_eq!(refs[3].task_idx, 0);
        assert_eq!(refs[3].sub_name.as_deref(), Some("backend › api"));
    }

    #[test]
    fn task_refs_for_nested_node_lists_direct_only() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, &vec![0, 0]);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].node, vec![0, 0]);
        assert_eq!(refs[0].task_idx, 0);
        assert_eq!(refs[0].sub_name, None);
    }

    #[test]
    fn task_refs_for_out_of_bounds_section() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, &vec![99]);
        assert!(refs.is_empty());
    }

    #[test]
    fn get_task_from_ref_subsection_task() {
        let list = sample_todo_list();
        let r = TaskRef {
            node: vec![0, 0, 0],
            task_idx: 0,
            sub_name: Some("api".to_string()),
        };
        let task = get_task_from_ref(&list, &r).unwrap();
        assert_eq!(task.text, "Fix endpoint");
    }

    #[test]
    fn get_task_from_ref_mut_modifies_task() {
        let mut list = sample_todo_list();
        let r = TaskRef {
            node: vec![0],
            task_idx: 0,
            sub_name: None,
        };
        let task = get_task_from_ref_mut(&mut list, &r).unwrap();
        task.is_done = true;
        task.text = "Modified".to_string();
        assert!(list.sections[0].tasks[0].is_done);
        assert_eq!(list.sections[0].tasks[0].text, "Modified");
    }

    #[test]
    fn get_task_from_ref_mut_nested_subsection() {
        let mut list = sample_todo_list();
        let r = TaskRef {
            node: vec![0, 0, 0],
            task_idx: 0,
            sub_name: Some("api".to_string()),
        };
        let task = get_task_from_ref_mut(&mut list, &r).unwrap();
        task.text = "Updated endpoint".to_string();
        assert_eq!(
            list.sections[0].children[0].children[0].tasks[0].text,
            "Updated endpoint"
        );
    }

    #[test]
    fn insert_section_top_level() {
        let mut list = sample_todo_list();
        let path = insert_section(&mut list, None, 1, Section::new("inserted")).unwrap();
        assert_eq!(path, vec![1]);
        assert_eq!(list.sections.len(), 3);
        assert_eq!(list.sections[1].name, "inserted");
    }

    #[test]
    fn insert_section_nested() {
        let mut list = sample_todo_list();
        let path = insert_section(&mut list, Some(&vec![0]), 0, Section::new("child")).unwrap();
        assert_eq!(path, vec![0, 0]);
        assert_eq!(list.sections[0].children[0].name, "child");
        assert_eq!(list.sections[0].children[1].name, "backend");
    }

    #[test]
    fn remove_section_nested() {
        let mut list = sample_todo_list();
        let removed = remove_section(&mut list, &vec![0, 0]).unwrap();
        assert_eq!(removed.name, "backend");
        assert!(list.sections[0].children.is_empty());
    }

    #[test]
    fn child_count_helper() {
        let list = sample_todo_list();
        assert_eq!(child_count(&list, None), 2);
        assert_eq!(child_count(&list, Some(&vec![0])), 1);
        assert_eq!(child_count(&list, Some(&vec![0, 0])), 1);
    }

    #[test]
    fn filter_restore_node_reselects_after_tree_rebuild() {
        let list = sample_todo_list();
        let mut app = AppState::new(list.sections.len());
        app.tree_items = build_tree_items(&list, &app.mode);
        app.filter_restore_node = Some(vec![0, 0]);

        app.tree_items = build_visible_tree_items(&list, &Mode::Normal, "backend");
        if let Some(node) = app.filter_restore_node.take()
            && let Some(pos) = app
                .tree_items
                .iter()
                .position(|item| matches!(item, TreeItem::Node(p) if p == &node))
        {
            app.left_state.select(Some(pos));
            app.right_state.select(Some(0));
        }

        assert_eq!(app.left_state.selected(), Some(1));
        assert_eq!(app.right_state.selected(), Some(0));
    }

    #[test]
    fn filter_restore_task_reselects_after_tree_rebuild() {
        let list = sample_todo_list();
        let mut app = AppState::new(list.sections.len());
        app.tree_items = build_tree_items(&list, &app.mode);
        app.filter_restore_node = Some(vec![0]);
        app.filter_restore_task_node = Some(vec![0, 0]);
        app.filter_restore_task_idx = Some(0);

        app.tree_items = build_visible_tree_items(&list, &Mode::Normal, "backend");
        if let Some(node) = app.filter_restore_node.take()
            && let Some(pos) = app
                .tree_items
                .iter()
                .position(|item| matches!(item, TreeItem::Node(p) if p == &node))
        {
            app.left_state.select(Some(pos));
            if let (Some(task_node), Some(task_idx)) = (
                app.filter_restore_task_node.take(),
                app.filter_restore_task_idx.take(),
            ) {
                let refs = get_task_refs_filtered(&list, &node, "");
                if let Some(new_pos) = refs
                    .iter()
                    .position(|r| r.node == task_node && r.task_idx == task_idx)
                {
                    app.right_state.select(Some(new_pos));
                } else {
                    app.right_state.select(Some(0));
                }
            } else {
                app.right_state.select(Some(0));
            }
        }

        assert_eq!(app.left_state.selected(), Some(0));
        assert_eq!(app.right_state.selected(), Some(2));
    }
}
