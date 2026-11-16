//! Terminal lifecycle and the main event loop.
//!
//! `run_tui` owns raw mode, the alternate screen, and mouse capture. A guard
//! restores all three on normal exit, on error, and during panic unwinding.
//! `run_engine` owns the document and UI state but no terminal setup.

pub mod input;
pub mod render;
pub mod state;
pub mod task_ref;
pub mod tree;

use crate::parser::parse_file;
use crate::task::TodoList;
use crate::tui::state::{AppState, Focus, Mode, TreeItem};
use crate::tui::task_ref::{TaskRef, get_task_refs_filtered};
use crate::tui::tree::{
    build_visible_tree_items, clamp_left_selection, clamp_right_selection, selected_node,
};
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use std::io::{self, Stdout};
use std::path::Path;

/// Width of the section panel as a percentage of terminal width.
pub const LEFT_PANEL_PERCENT: u16 = 27;

/// Help popup width percentage.
pub const HELP_POPUP_WIDTH_PERCENT: u16 = 55;

/// Help popup height in rows.
pub const HELP_POPUP_HEIGHT: u16 = 34;

/// Run the TUI until the user quits.
pub fn run_tui(path: &Path) -> Result<()> {
    let mut session = TerminalSession::enter()?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_engine(&mut session.terminal, path)
    }));
    drop(session);

    match result {
        Ok(result) => result,
        Err(panic) => Err(anyhow::anyhow!(
            "TUI panicked: {}",
            panic_message(panic.as_ref())
        )),
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Terminal state that must be restored on every exit path.
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;

        let terminal = (|| -> Result<Terminal<CrosstermBackend<Stdout>>> {
            let mut out = io::stdout();
            execute!(out, EnterAlternateScreen).context("enter alternate screen")?;
            execute!(out, event::EnableMouseCapture).context("enable mouse capture")?;
            Terminal::new(CrosstermBackend::new(out)).context("create terminal")
        })();

        match terminal {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                restore_terminal(None);
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        restore_terminal(Some(&mut self.terminal));
    }
}

fn restore_terminal(terminal: Option<&mut Terminal<CrosstermBackend<Stdout>>>) {
    let _ = disable_raw_mode();
    match terminal {
        Some(terminal) => {
            let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
            let _ = execute!(terminal.backend_mut(), event::DisableMouseCapture);
            let _ = terminal.show_cursor();
        }
        None => {
            let mut out = io::stdout();
            let _ = execute!(out, LeaveAlternateScreen);
            let _ = execute!(out, event::DisableMouseCapture);
        }
    }
}

fn run_engine(terminal: &mut Terminal<CrosstermBackend<Stdout>>, path: &Path) -> Result<()> {
    let mut todo_list = parse_file(path)?;
    let mut app = AppState::new(todo_list.sections.len());

    loop {
        refresh_view(&mut app, &todo_list);
        terminal.draw(|frame| render::draw_ui(frame, &todo_list, &mut app))?;

        let (columns, rows) = size()?;
        let area = Rect::new(0, 0, columns, rows);

        match event::read()? {
            Event::Key(key)
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == event::KeyCode::Char('c') =>
            {
                break;
            }
            Event::Key(key) => {
                if input::handle_key(&mut app, &mut todo_list, path, key.code, key.modifiers)? {
                    break;
                }
            }
            Event::Mouse(mouse) => handle_mouse(&mut app, &todo_list, area, mouse),
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

/// Rebuild derived view state without mutating the document.
fn refresh_view(app: &mut AppState, todo_list: &TodoList) {
    app.tree_items = build_visible_tree_items(todo_list, &app.mode, &app.filter_lower);

    restore_filter_selection(app, todo_list);

    let mode = app.mode.clone();
    if matches!(&mode, Mode::Filter)
        && let Some(anchor) = app.filter_anchor.clone()
        && select_path(app, &anchor)
    {
        clamp_right_selection_for_node(app, todo_list);
        return;
    }

    match mode {
        Mode::InputSection(input) if input.node.is_none() => {
            select_ghost(app);
        }
        Mode::InputSection(input) => {
            if let Some(node) = &input.node {
                select_path(app, node);
            }
        }
        Mode::InputTask(input) => {
            select_path(app, &input.node);
            let refs = get_task_refs_filtered(todo_list, &input.node, &app.filter_lower);
            let row = match input.editing_idx {
                Some(index) => index,
                None => input.insert_idx.unwrap_or(refs.len()),
            };
            if input.editing_idx.is_some() {
                if refs.is_empty() {
                    app.right_state.select(None);
                } else {
                    app.right_state.select(Some(row.min(refs.len() - 1)));
                }
            } else {
                // The row list includes the ghost row at the insertion point.
                app.right_state.select(Some(row.min(refs.len())));
            }
        }
        _ => {
            if app.filter_anchor.is_some() {
                app.filter_anchor = None;
            }
            clamp_left_selection(app);
            clamp_right_selection_for_node(app, todo_list);
        }
    }
}

fn restore_filter_selection(app: &mut AppState, todo_list: &TodoList) {
    let Some(snapshot) = app.filter_restore.take() else {
        return;
    };

    if let Some(node) = &snapshot.node {
        select_path(app, node);
    }

    if let Some(task) = snapshot.task {
        let refs = get_task_refs_filtered(todo_list, &task.node, &app.filter_lower);
        if let Some(position) = find_task_position(&refs, &task) {
            app.right_state.select(Some(position));
        } else {
            app.right_state.select(None);
        }
    }
}

fn find_task_position(refs: &[TaskRef], task: &TaskRef) -> Option<usize> {
    refs.iter()
        .position(|reference| reference.node == task.node && reference.task_idx == task.task_idx)
}

fn clamp_right_selection_for_node(app: &mut AppState, todo_list: &TodoList) {
    let row_count = selected_node(app).map_or(0, |node| {
        get_task_refs_filtered(todo_list, &node, &app.filter_lower).len()
    });
    clamp_right_selection(app, row_count);
}

fn select_path(app: &mut AppState, path: &[usize]) -> bool {
    if let Some(position) = app
        .tree_items
        .iter()
        .position(|item| matches!(item, TreeItem::Node(p) if p.as_slice() == path))
    {
        app.left_state.select(Some(position));
        true
    } else {
        false
    }
}

fn select_ghost(app: &mut AppState) {
    if let Some(position) = app
        .tree_items
        .iter()
        .position(|item| matches!(item, TreeItem::Ghost(_)))
    {
        app.left_state.select(Some(position));
    } else {
        clamp_left_selection(app);
    }
}

fn handle_mouse(app: &mut AppState, todo_list: &TodoList, area: Rect, event: MouseEvent) {
    let MouseEvent {
        kind, column, row, ..
    } = event;

    if matches!(app.mode, Mode::Help) {
        handle_help_mouse(app, area, kind, column, row);
        return;
    }
    if app.mode.is_text_input() {
        app.clear_pending();
        return;
    }

    match kind {
        MouseEventKind::ScrollUp => scroll(app, todo_list, -1),
        MouseEventKind::ScrollDown => scroll(app, todo_list, 1),
        MouseEventKind::Down(MouseButton::Left) => {
            handle_click(app, todo_list, area, column, row);
        }
        _ => {}
    }
}

fn handle_help_mouse(app: &mut AppState, area: Rect, kind: MouseEventKind, column: u16, row: u16) {
    match kind {
        MouseEventKind::ScrollUp => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }
        MouseEventKind::ScrollDown => {
            app.help_scroll = app.help_scroll.saturating_add(1);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let popup = help_popup(area);
            let inside = column >= popup.x
                && column < popup.x.saturating_add(popup.width)
                && row >= popup.y
                && row < popup.y.saturating_add(popup.height);
            if !inside {
                app.mode = Mode::Normal;
            }
        }
        _ => {}
    }
}

fn help_popup(area: Rect) -> Rect {
    let width = percentage(area.width, HELP_POPUP_WIDTH_PERCENT);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(HELP_POPUP_HEIGHT)) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: HELP_POPUP_HEIGHT.min(area.height),
    }
}

fn handle_click(app: &mut AppState, todo_list: &TodoList, area: Rect, column: u16, row: u16) {
    let Some(row_index) = row.checked_sub(1).map(usize::from) else {
        return;
    };
    if row_index + 2 >= area.height as usize {
        return;
    }

    let left_width = percentage(area.width, LEFT_PANEL_PERCENT);
    if column < left_width {
        app.focus = Focus::Left;
        if row_index < app.tree_items.len() {
            app.left_state.select(Some(row_index));
        }
        return;
    }

    app.focus = Focus::Right;
    if let Some(node) = selected_node(app) {
        let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
        if row_index < refs.len() {
            app.right_state.select(Some(row_index));
            return;
        }
    }
    clamp_right_selection_for_node(app, todo_list);
}

fn scroll(app: &mut AppState, todo_list: &TodoList, delta: isize) {
    match app.focus {
        Focus::Left => {
            if app.tree_items.is_empty() {
                app.left_state.select(None);
                return;
            }
            let current = app.left_state.selected().unwrap_or(0) as isize;
            let max = app.tree_items.len().saturating_sub(1) as isize;
            app.left_state
                .select(Some(current.saturating_add(delta).clamp(0, max) as usize));
        }
        Focus::Right => {
            let Some(node) = selected_node(app) else {
                app.right_state.select(None);
                return;
            };
            let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
            if refs.is_empty() {
                app.right_state.select(None);
                return;
            }
            let current = app.right_state.selected().unwrap_or(0) as isize;
            let max = refs.len().saturating_sub(1) as isize;
            app.right_state
                .select(Some(current.saturating_add(delta).clamp(0, max) as usize));
        }
    }
}

/// Percentage of a `u16` dimension without integer overflow.
pub fn percentage(total: u16, percent: u16) -> u16 {
    (u32::from(total) * u32::from(percent) / 100) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_str;

    fn list() -> TodoList {
        parse_str("# main\n## sub\n- [ ] A\n## other\n- [ ] B\n")
    }

    #[test]
    fn filter_anchor_keeps_selected_node_while_typing() {
        let list = list();
        let mut app = AppState::new(list.sections.len());
        app.mode = Mode::Filter;
        app.filter_anchor = Some(vec![0, 0]);
        app.filter = "sub".into();
        app.filter_lower = "sub".into();

        refresh_view(&mut app, &list);

        assert_eq!(selected_node(&app), Some(vec![0, 0]));
    }

    #[test]
    fn filter_anchor_falls_back_when_node_disappears() {
        let list = list();
        let mut app = AppState::new(list.sections.len());
        app.mode = Mode::Filter;
        app.filter_anchor = Some(vec![0, 0]);
        app.filter = "other".into();
        app.filter_lower = "other".into();

        refresh_view(&mut app, &list);

        assert!(
            app.left_state
                .selected()
                .is_some_and(|i| i < app.tree_items.len())
        );
    }

    #[test]
    fn stale_selection_is_clamped() {
        let list = list();
        let mut app = AppState::new(list.sections.len());
        app.left_state.select(Some(99));
        app.right_state.select(Some(99));

        refresh_view(&mut app, &list);

        assert!(
            app.left_state
                .selected()
                .is_some_and(|i| i < app.tree_items.len())
        );
        assert!(app.right_state.selected().is_some());
    }

    #[test]
    fn filter_restore_selects_node_and_task() {
        let list = parse_str("# main\n- [ ] A\n- [ ] B\n");
        let mut app = AppState::new(list.sections.len());
        app.focus = Focus::Right;
        app.filter_restore = Some(state::SelectionSnapshot {
            node: Some(vec![0]),
            task: Some(TaskRef {
                node: vec![0],
                task_idx: 1,
                sub_name: None,
            }),
        });

        refresh_view(&mut app, &list);

        assert_eq!(selected_node(&app), Some(vec![0]));
        assert_eq!(app.right_state.selected(), Some(1));
    }

    #[test]
    fn task_input_selects_node_and_ghost_position() {
        let list = list();
        let mut app = AppState::new(list.sections.len());
        app.mode = Mode::InputTask(state::TaskInput {
            editing_idx: None,
            insert_idx: Some(0),
            above: false,
            node: vec![0, 1],
            field: state::TextField::new("new"),
        });

        refresh_view(&mut app, &list);

        assert_eq!(selected_node(&app), Some(vec![0, 1]));
        assert_eq!(app.right_state.selected(), Some(0));
    }

    #[test]
    fn task_input_at_end_selects_the_ghost_row() {
        let list = list();
        let mut app = AppState::new(list.sections.len());
        app.mode = Mode::InputTask(state::TaskInput {
            editing_idx: None,
            insert_idx: Some(2),
            above: false,
            node: vec![0],
            field: state::TextField::new("new"),
        });

        refresh_view(&mut app, &list);

        assert_eq!(app.right_state.selected(), Some(2));
    }

    #[test]
    fn percentage_does_not_overflow() {
        assert_eq!(percentage(u16::MAX, 55), 36044);
        assert_eq!(percentage(100, 27), 27);
    }
}
