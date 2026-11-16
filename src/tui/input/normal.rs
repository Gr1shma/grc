//! Normal-mode key handling.
//!
//! Navigation, two-key commands, filtering, and document mutations live here.
//! Every mutation writes the document immediately; there is no unsaved state.

use crate::parser::write_file;
use crate::task::{
    NodePath, Section, TodoList, child_count, get_node, get_node_mut, insert_section,
    remove_section, task_count,
};
use crate::tui::state::{
    AppState, ClipboardItem, Focus, Mode, PendingKey, SectionInput, SelectionSnapshot, TaskInput,
    TextField,
};
use crate::tui::task_ref::{
    TaskRef, get_task_from_ref, get_task_from_ref_mut, get_task_refs_filtered,
};
use crate::tui::tree::{
    build_visible_tree_items, clamp_left_selection, clamp_right_selection, node_level, node_name,
    selected_node,
};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::path::Path;

/// Handle a key in normal navigation mode.
///
/// Returns `true` when the application should quit.
pub fn handle_normal(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
) -> Result<bool> {
    if handle_pending(app, todo_list, path, code)? {
        return Ok(false);
    }

    match code {
        KeyCode::Char('?') => {
            app.mode = Mode::Help;
            return Ok(false);
        }
        KeyCode::Char(':') if app.focus == Focus::Right => {
            sort_tasks_by_due(app, todo_list, path)?;
            return Ok(false);
        }
        KeyCode::Char('/') => {
            app.filter_anchor = selected_node(app);
            app.mode = Mode::Filter;
            return Ok(false);
        }
        KeyCode::Esc if !app.filter.is_empty() => {
            remember_selection(app, todo_list);
            clear_filter(app);
            return Ok(false);
        }
        _ => {}
    }

    match app.focus {
        Focus::Left => handle_left(app, todo_list, path, code),
        Focus::Right => handle_right(app, todo_list, path, code),
    }
}

/// Handle the second key of `dd`, `gg`, or `yy`.
///
/// The prefix is consumed by every key: an unrelated key cancels the command
/// and is not also executed, matching the previous behaviour.
fn handle_pending(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
) -> Result<bool> {
    let pending = std::mem::replace(&mut app.pending, PendingKey::None);
    match pending {
        PendingKey::None => Ok(false),
        PendingKey::Delete => {
            if code == KeyCode::Char('d') {
                match app.focus {
                    Focus::Left => delete_section(app, todo_list, path)?,
                    Focus::Right => delete_task(app, todo_list, path)?,
                }
            }
            Ok(true)
        }
        PendingKey::Goto => {
            if code == KeyCode::Char('g') {
                jump_to_top(app, todo_list);
            }
            Ok(true)
        }
        PendingKey::Yank => {
            if code == KeyCode::Char('y') {
                match app.focus {
                    Focus::Left => yank_section(app, todo_list),
                    Focus::Right => yank_task(app, todo_list),
                }
            }
            Ok(true)
        }
    }
}

fn jump_to_top(app: &mut AppState, todo_list: &TodoList) {
    match app.focus {
        Focus::Left => {
            if app.tree_items.is_empty() {
                app.left_state.select(None);
            } else {
                app.left_state.select(Some(0));
            }
            app.right_state.select(Some(0));
        }
        Focus::Right => {
            if let Some(node) = selected_node(app) {
                let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                app.right_state.select((!refs.is_empty()).then_some(0));
            }
        }
    }
}

/// Handle typing in the live-filter mode.
pub fn handle_filter(app: &mut AppState, todo_list: &TodoList, code: KeyCode) -> Result<()> {
    match code {
        KeyCode::Esc => {
            remember_selection(app, todo_list);
            clear_filter(app);
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.filter_anchor = None;
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            let mut filter = app.filter.clone();
            filter.pop();
            app.set_filter(filter);
        }
        KeyCode::Char(character) => {
            let mut filter = app.filter.clone();
            filter.push(character);
            app.set_filter(filter);
        }
        _ => {}
    }
    Ok(())
}

fn remember_selection(app: &mut AppState, todo_list: &TodoList) {
    let node = selected_node(app);
    let task = if app.focus == Focus::Right {
        node.as_ref().and_then(|node| {
            let refs = get_task_refs_filtered(todo_list, node, &app.filter_lower);
            refs.get(app.right_state.selected().unwrap_or(0)).cloned()
        })
    } else {
        None
    };
    app.filter_restore = Some(SelectionSnapshot { node, task });
    app.filter_anchor = None;
}

fn clear_filter(app: &mut AppState) {
    app.clear_filter();
}

fn handle_left(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
) -> Result<bool> {
    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('j') | KeyCode::Down => move_left_down(app),
        KeyCode::Char('k') | KeyCode::Up => move_left_up(app),
        KeyCode::Char('l') | KeyCode::Tab | KeyCode::Enter => {
            if selected_node(app).is_some() {
                app.focus = Focus::Right;
            }
        }
        KeyCode::Char('a' | 'A' | 'o' | 'O') => {
            app.mode = Mode::InputSection(new_section_input(todo_list, app, code));
        }
        KeyCode::Char('i') => {
            if let Some(node) = selected_node(app) {
                let name = node_name(todo_list, &node);
                let level = node_level(todo_list, &node);
                app.mode = Mode::InputSection(SectionInput {
                    node: Some(node),
                    parent: None,
                    insert_idx: None,
                    level,
                    field: TextField::new(name),
                });
            }
        }
        KeyCode::Char('d') => app.pending = PendingKey::Delete,
        KeyCode::Char('g') => app.pending = PendingKey::Goto,
        KeyCode::Char('y') => app.pending = PendingKey::Yank,
        KeyCode::Char('p') => paste_clipboard(app, todo_list, path, false)?,
        KeyCode::Char('P') => paste_clipboard(app, todo_list, path, true)?,
        KeyCode::Char('G') => move_left_bottom(app),
        _ => {}
    }
    Ok(false)
}

fn move_left_down(app: &mut AppState) {
    if app.tree_items.is_empty() {
        app.left_state.select(None);
        return;
    }
    let current = app.left_state.selected().unwrap_or(0);
    app.left_state
        .select(Some((current + 1).min(app.tree_items.len() - 1)));
    app.right_state.select(Some(0));
}

fn move_left_up(app: &mut AppState) {
    if app.tree_items.is_empty() {
        app.left_state.select(None);
        return;
    }
    let current = app.left_state.selected().unwrap_or(0);
    app.left_state.select(Some(current.saturating_sub(1)));
    app.right_state.select(Some(0));
}

fn move_left_bottom(app: &mut AppState) {
    if app.tree_items.is_empty() {
        app.left_state.select(None);
    } else {
        app.left_state.select(Some(app.tree_items.len() - 1));
    }
    app.right_state.select(Some(0));
}

fn new_section_input(todo_list: &TodoList, app: &AppState, code: KeyCode) -> SectionInput {
    let field = TextField::default();
    let Some(node) = selected_node(app) else {
        return SectionInput {
            node: None,
            parent: None,
            insert_idx: (code == KeyCode::Char('a')).then_some(0),
            level: 1,
            field,
        };
    };

    let depth = node.len();
    let level = node_level(todo_list, &node);

    let (parent, insert_idx, target_level) = match code {
        KeyCode::Char('a') => (Some(node), Some(0), level + 1),
        KeyCode::Char('A') if depth == 1 => (None, Some(node[0]), 1),
        KeyCode::Char('A') => {
            let parent = node[..depth - 2].to_vec();
            let parent_level = if parent.is_empty() {
                0
            } else {
                node_level(todo_list, &parent)
            };
            let insert_idx = node[depth - 2];
            let parent = (!parent.is_empty()).then_some(parent);
            (parent, Some(insert_idx), parent_level + 1)
        }
        KeyCode::Char('o') => {
            let parent = (depth > 1).then(|| node[..depth - 1].to_vec());
            (parent, Some(node[depth - 1] + 1), level)
        }
        KeyCode::Char('O') => {
            let parent = (depth > 1).then(|| node[..depth - 1].to_vec());
            (parent, Some(node[depth - 1]), level)
        }
        _ => (None, None, 1),
    };

    SectionInput {
        node: None,
        parent,
        insert_idx,
        level: target_level,
        field,
    }
}

fn handle_right(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
) -> Result<bool> {
    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Tab => {
            app.focus = Focus::Left;
        }
        KeyCode::Char('j') | KeyCode::Down => move_right_down(app, todo_list),
        KeyCode::Char('k') | KeyCode::Up => move_right_up(app, todo_list),
        KeyCode::Char(' ' | 'x') | KeyCode::Enter => toggle_task(app, todo_list, path)?,
        KeyCode::Char('a' | 'o' | 'O' | 'A') => {
            if let Some(node) = selected_node(app) {
                app.mode = Mode::InputTask(new_task_input(app, node, code));
            }
        }
        KeyCode::Char('i') => edit_task(app, todo_list),
        KeyCode::Char('t') => edit_due(app, todo_list),
        KeyCode::Char('d') => app.pending = PendingKey::Delete,
        KeyCode::Char('g') => app.pending = PendingKey::Goto,
        KeyCode::Char('y') => app.pending = PendingKey::Yank,
        KeyCode::Char('p') => paste_clipboard(app, todo_list, path, false)?,
        KeyCode::Char('P') => paste_clipboard(app, todo_list, path, true)?,
        KeyCode::Char('G') => move_right_bottom(app, todo_list),
        _ => {}
    }
    Ok(false)
}

fn move_right_down(app: &mut AppState, todo_list: &TodoList) {
    let Some(node) = selected_node(app) else {
        app.right_state.select(None);
        return;
    };
    let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
    if refs.is_empty() {
        app.right_state.select(None);
    } else {
        let current = app.right_state.selected().unwrap_or(0);
        app.right_state
            .select(Some((current + 1).min(refs.len() - 1)));
    }
}

fn move_right_up(app: &mut AppState, todo_list: &TodoList) {
    let Some(node) = selected_node(app) else {
        app.right_state.select(None);
        return;
    };
    let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
    if refs.is_empty() {
        app.right_state.select(None);
    } else {
        let current = app.right_state.selected().unwrap_or(0);
        app.right_state
            .select(Some(current.saturating_sub(1).min(refs.len() - 1)));
    }
}

fn move_right_bottom(app: &mut AppState, todo_list: &TodoList) {
    let Some(node) = selected_node(app) else {
        app.right_state.select(None);
        return;
    };
    let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
    app.right_state
        .select((!refs.is_empty()).then_some(refs.len() - 1));
}

fn toggle_task(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    let Some(reference) = current_task_ref(app, todo_list) else {
        return Ok(());
    };
    let Some(task) = get_task_from_ref_mut(todo_list, &reference) else {
        return Ok(());
    };
    task.is_done = !task.is_done;
    write_file(path, todo_list)
}

fn new_task_input(app: &AppState, node: NodePath, code: KeyCode) -> TaskInput {
    let current = app.right_state.selected().unwrap_or(0);
    let (insert_idx, above) = match code {
        KeyCode::Char('a' | 'o') => (Some(current + 1), false),
        KeyCode::Char('O') => (Some(current), true),
        KeyCode::Char('A') => (None, false),
        _ => (None, false),
    };
    TaskInput {
        editing_idx: None,
        insert_idx,
        above,
        node,
        field: TextField::default(),
    }
}

fn edit_task(app: &mut AppState, todo_list: &TodoList) {
    let current = app.right_state.selected().unwrap_or(0);
    let Some(reference) = current_task_ref(app, todo_list) else {
        return;
    };
    let Some(task) = get_task_from_ref(todo_list, &reference) else {
        return;
    };
    let text = task.text.clone();
    app.mode = Mode::InputTask(TaskInput {
        editing_idx: Some(current),
        insert_idx: None,
        above: false,
        node: reference.node,
        field: TextField::new(text),
    });
}

fn edit_due(app: &mut AppState, todo_list: &TodoList) {
    let Some(reference) = current_task_ref(app, todo_list) else {
        return;
    };
    let Some(task) = get_task_from_ref(todo_list, &reference) else {
        return;
    };
    let existing = task
        .due
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    app.mode = Mode::InputDue(crate::tui::state::DueInput {
        target: reference,
        field: TextField::new(existing),
    });
}

fn current_task_ref(app: &AppState, todo_list: &TodoList) -> Option<TaskRef> {
    let node = selected_node(app)?;
    let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
    refs.get(app.right_state.selected().unwrap_or(0)).cloned()
}

fn delete_task(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    let Some(selected) = selected_node(app) else {
        return Ok(());
    };
    let Some(reference) = current_task_ref(app, todo_list) else {
        return Ok(());
    };
    let Some(task) = get_task_from_ref(todo_list, &reference).cloned() else {
        return Ok(());
    };
    let Some(section) = get_node_mut(todo_list, &reference.node) else {
        return Ok(());
    };
    section.tasks.remove(reference.task_idx);
    app.clipboard = Some(ClipboardItem::Task(task));
    write_file(path, todo_list)?;

    let new_refs = get_task_refs_filtered(todo_list, &selected, &app.filter_lower);
    if new_refs.is_empty() {
        app.right_state.select(None);
        app.focus = Focus::Left;
    } else {
        let current = app.right_state.selected().unwrap_or(0);
        app.right_state
            .select(Some(current.min(new_refs.len() - 1)));
    }
    Ok(())
}

fn delete_section(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    let Some(node) = selected_node(app) else {
        return Ok(());
    };
    let Some(removed) = remove_section(todo_list, &node) else {
        return Ok(());
    };
    let removed_index = app.left_state.selected().unwrap_or(0);
    app.clipboard = Some(ClipboardItem::Section(removed));
    write_file(path, todo_list)?;

    app.tree_items = build_visible_tree_items(todo_list, &Mode::Normal, &app.filter_lower);
    if app.tree_items.is_empty() {
        app.left_state.select(None);
    } else {
        let index = removed_index
            .saturating_sub(1)
            .min(app.tree_items.len() - 1);
        app.left_state.select(Some(index));
    }
    app.right_state.select(None);
    app.filter_anchor = None;
    Ok(())
}

fn yank_task(app: &mut AppState, todo_list: &TodoList) {
    if let Some(reference) = current_task_ref(app, todo_list)
        && let Some(task) = get_task_from_ref(todo_list, &reference)
    {
        app.clipboard = Some(ClipboardItem::Task(task.clone()));
    }
}

fn yank_section(app: &mut AppState, todo_list: &TodoList) {
    if let Some(node) = selected_node(app)
        && let Some(section) = get_node(todo_list, &node)
    {
        app.clipboard = Some(ClipboardItem::Section(section.clone()));
    }
}

/// Paste the clipboard below (`p`) or above (`P`) the current item.
pub fn paste_clipboard(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    above: bool,
) -> Result<()> {
    let Some(clipboard) = app.clipboard.clone() else {
        return Ok(());
    };
    match clipboard {
        ClipboardItem::Task(task) => paste_task(app, todo_list, path, task, above),
        ClipboardItem::Section(section) => paste_section(app, todo_list, path, section, above),
    }
}

fn paste_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    task: crate::task::Task,
    above: bool,
) -> Result<()> {
    let Some(selected) = selected_node(app) else {
        return Ok(());
    };
    let refs = get_task_refs_filtered(todo_list, &selected, &app.filter_lower);
    let current = app.right_state.selected().unwrap_or(0);

    let (target_node, insert_index) = if app.focus == Focus::Right {
        refs.get(current).map_or_else(
            || (selected.clone(), task_count(todo_list, &selected)),
            |reference| {
                let base = reference.task_idx;
                (reference.node.clone(), if above { base } else { base + 1 })
            },
        )
    } else {
        (
            selected.clone(),
            if above {
                0
            } else {
                task_count(todo_list, &selected)
            },
        )
    };

    let Some(section) = get_node_mut(todo_list, &target_node) else {
        return Ok(());
    };
    let insert_index = insert_index.min(section.tasks.len());
    section.tasks.insert(insert_index, task);
    write_file(path, todo_list)?;

    app.tree_items = build_visible_tree_items(todo_list, &app.mode, &app.filter_lower);
    if app.focus == Focus::Right {
        let new_refs = get_task_refs_filtered(todo_list, &selected, &app.filter_lower);
        if let Some(position) = new_refs.iter().position(|reference| {
            reference.node.as_slice() == target_node.as_slice()
                && reference.task_idx == insert_index
        }) {
            app.right_state.select(Some(position));
        } else {
            clamp_right_selection(app, new_refs.len());
        }
    }
    Ok(())
}

fn paste_section(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    section: Section,
    above: bool,
) -> Result<()> {
    let (parent, index, level) = match selected_node(app) {
        Some(node) => {
            let depth = node.len();
            let level = node_level(todo_list, &node);
            let parent = (depth > 1).then(|| node[..depth - 1].to_vec());
            let index = if above {
                node[depth - 1]
            } else {
                (node[depth - 1] + 1).min(child_count(todo_list, parent.as_ref()))
            };
            (parent, index, level)
        }
        None => (None, if above { 0 } else { todo_list.sections.len() }, 1),
    };

    let Some(new_path) = insert_section(todo_list, parent.as_ref(), index, section, level) else {
        return Ok(());
    };
    write_file(path, todo_list)?;
    app.mode = Mode::Normal;
    app.tree_items = build_visible_tree_items(todo_list, &Mode::Normal, &app.filter_lower);
    if let Some(position) = app
        .tree_items
        .iter()
        .position(|item| matches!(item, crate::tui::state::TreeItem::Node(p) if p == &new_path))
    {
        app.left_state.select(Some(position));
    } else {
        clamp_left_selection(app);
    }
    app.right_state.select(None);
    Ok(())
}

fn sort_tasks_by_due(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    let Some(node) = selected_node(app) else {
        return Ok(());
    };
    let Some(section) = get_node_mut(todo_list, &node) else {
        return Ok(());
    };
    section
        .tasks
        .sort_by_key(|task| (task.due.is_none(), task.due));
    write_file(path, todo_list)
}
