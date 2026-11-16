//! Normal-mode key handling.
//!
//! Navigation and mode changes mutate only ephemeral UI state. Document edits
//! are returned as [`Action`] values and applied centrally by the engine.

use crate::task::{NodePath, Section, TodoList, get_node};
use crate::tui::actions::Action;
use crate::tui::state::{
    AppState, ClipboardItem, Focus, Mode, PendingKey, SectionInput, SelectionSnapshot, TaskInput,
    TextField, TreeItem,
};
use crate::tui::task_ref::{TaskRef, get_task_from_ref, get_task_refs_filtered};
use crate::tui::tree::{node_level, node_name, selected_node};
use crossterm::event::{KeyCode, KeyModifiers};
use std::collections::HashSet;

/// Handle a key in normal navigation mode.
pub fn handle_normal(
    app: &mut AppState,
    todo_list: &TodoList,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Vec<Action> {
    if app.pending == PendingKey::None
        && let KeyCode::Char(character) = code
        && let Some(digit) = character.to_digit(10)
    {
        app.push_count_digit(digit as usize);
        return Vec::new();
    }

    if app.pending != PendingKey::None {
        let count = app.take_count();
        return handle_pending(app, todo_list, code, count);
    }

    // Pending-key commands keep the count until the second key (`3dd`).
    let pending_starter = matches!(code, KeyCode::Char('d' | 'g' | 'y' | 'z' | ']'));
    let count = if pending_starter {
        app.count.unwrap_or(1)
    } else {
        app.take_count()
    };
    match code {
        KeyCode::Char('r') if modifiers.contains(KeyModifiers::CONTROL) => {
            vec![Action::Redo]
        }
        KeyCode::Char('?') => {
            app.mode = Mode::Help;
            app.help_scroll = 0;
            Vec::new()
        }
        KeyCode::Char(':') if app.focus == Focus::Right => selected_node(app)
            .map(|node| vec![Action::Sort { node }])
            .unwrap_or_default(),
        KeyCode::Char('/') => {
            app.filter_anchor = selected_node(app);
            app.mode = Mode::Filter;
            Vec::new()
        }
        KeyCode::Esc if !app.filter.is_empty() => {
            remember_selection(app, todo_list);
            app.clear_filter();
            Vec::new()
        }
        _ => match app.focus {
            Focus::Left => handle_left(app, todo_list, code, count),
            Focus::Right => handle_right(app, todo_list, code, count),
        },
    }
}

fn handle_pending(
    app: &mut AppState,
    todo_list: &TodoList,
    code: KeyCode,
    count: usize,
) -> Vec<Action> {
    let pending = std::mem::replace(&mut app.pending, PendingKey::None);
    match pending {
        PendingKey::None => Vec::new(),
        PendingKey::Delete => {
            if code != KeyCode::Char('d') {
                return Vec::new();
            }
            match app.focus {
                Focus::Left => selected_node(app)
                    .map(|path| vec![Action::DeleteSection { path }])
                    .unwrap_or_default(),
                Focus::Right => {
                    let targets = selected_task_targets(app, todo_list, count);
                    (!targets.is_empty())
                        .then_some(vec![Action::DeleteTasks { targets }])
                        .unwrap_or_default()
                }
            }
        }
        PendingKey::Goto => {
            if code == KeyCode::Char('g') {
                jump_to_top(app, todo_list);
            }
            Vec::new()
        }
        PendingKey::Yank => {
            if code == KeyCode::Char('y') {
                match app.focus {
                    Focus::Left => yank_section(app, todo_list),
                    Focus::Right => yank_task(app, todo_list),
                }
            }
            Vec::new()
        }
        PendingKey::Fold => {
            handle_fold(app, todo_list, code);
            Vec::new()
        }
        PendingKey::Overdue => {
            if code == KeyCode::Char('d') {
                jump_to_next_overdue(app, todo_list);
            }
            Vec::new()
        }
    }
}

fn handle_left(
    app: &mut AppState,
    todo_list: &TodoList,
    code: KeyCode,
    count: usize,
) -> Vec<Action> {
    match code {
        KeyCode::Char('q') => return vec![Action::Quit],
        KeyCode::Char('j') | KeyCode::Down => move_left(app, count as isize),
        KeyCode::Char('k') | KeyCode::Up => move_left(app, -(count as isize)),
        KeyCode::Char('l') | KeyCode::Tab | KeyCode::Enter => {
            if selected_node(app).is_some() {
                app.focus = Focus::Right;
            }
        }
        KeyCode::Char('a' | 'A' | 'o' | 'O') => {
            if let Some(node) = selected_node(app) {
                app.folded.remove(&node);
            }
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
        KeyCode::Char('z') => app.pending = PendingKey::Fold,
        KeyCode::Char(']') => app.pending = PendingKey::Overdue,
        KeyCode::Char('p') => return vec![Action::Paste { above: false }],
        KeyCode::Char('P') => return vec![Action::Paste { above: true }],
        KeyCode::Char('G') => move_left_bottom(app),
        KeyCode::Char('J') | KeyCode::Char('K') => {
            if let Some(path) = selected_node(app) {
                let delta = if code == KeyCode::Char('J') { 1 } else { -1 };
                return vec![Action::MoveSection { path, delta }];
            }
        }
        KeyCode::Char('u') => return vec![Action::Undo],
        KeyCode::Char('H') => toggle_hide_done(app),
        _ => {}
    }
    Vec::new()
}

fn handle_right(
    app: &mut AppState,
    todo_list: &TodoList,
    code: KeyCode,
    count: usize,
) -> Vec<Action> {
    match code {
        KeyCode::Char('q') => return vec![Action::Quit],
        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Tab => app.focus = Focus::Left,
        KeyCode::Char('j') | KeyCode::Down => move_right(app, todo_list, count as isize),
        KeyCode::Char('k') | KeyCode::Up => move_right(app, todo_list, -(count as isize)),
        KeyCode::Char(' ' | 'x') | KeyCode::Enter => {
            if let Some(target) = current_task_ref(app, todo_list) {
                return vec![Action::ToggleTask { target }];
            }
        }
        KeyCode::Char('a' | 'o' | 'O' | 'A') => {
            if let Some(node) = selected_node(app) {
                app.folded.remove(&node);
                app.mode = Mode::InputTask(new_task_input(app, node, code));
            }
        }
        KeyCode::Char('i') => edit_task(app, todo_list),
        KeyCode::Char('t') => edit_due(app, todo_list),
        KeyCode::Char('d') => app.pending = PendingKey::Delete,
        KeyCode::Char('g') => app.pending = PendingKey::Goto,
        KeyCode::Char('y') => app.pending = PendingKey::Yank,
        KeyCode::Char('z') => app.pending = PendingKey::Fold,
        KeyCode::Char(']') => app.pending = PendingKey::Overdue,
        KeyCode::Char('p') => return vec![Action::Paste { above: false }],
        KeyCode::Char('P') => return vec![Action::Paste { above: true }],
        KeyCode::Char('G') => move_right_bottom(app, todo_list),
        KeyCode::Char('J') | KeyCode::Char('K') => {
            if let Some(target) = current_task_ref(app, todo_list) {
                let delta = if code == KeyCode::Char('J') { 1 } else { -1 };
                return vec![Action::MoveTask { target, delta }];
            }
        }
        KeyCode::Char('u') => return vec![Action::Undo],
        KeyCode::Char('H') => toggle_hide_done(app),
        _ => {}
    }
    Vec::new()
}

fn move_left(app: &mut AppState, delta: isize) {
    if app.tree_items.is_empty() {
        app.left_state.select(None);
        return;
    }
    let current = app.left_state.selected().unwrap_or(0) as isize;
    let max = app.tree_items.len().saturating_sub(1) as isize;
    app.left_state
        .select(Some(current.saturating_add(delta).clamp(0, max) as usize));
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

fn refs_for(app: &AppState, todo_list: &TodoList, node: &NodePath) -> Vec<TaskRef> {
    get_task_refs_filtered(todo_list, node, &app.filter_lower, app.hide_done)
}

fn move_right(app: &mut AppState, todo_list: &TodoList, delta: isize) {
    let Some(node) = selected_node(app) else {
        app.right_state.select(None);
        return;
    };
    let refs = refs_for(app, todo_list, &node);
    if refs.is_empty() {
        app.right_state.select(None);
    } else {
        let current = app.right_state.selected().unwrap_or(0) as isize;
        let max = refs.len().saturating_sub(1) as isize;
        app.right_state
            .select(Some(current.saturating_add(delta).clamp(0, max) as usize));
    }
}

fn move_right_bottom(app: &mut AppState, todo_list: &TodoList) {
    let Some(node) = selected_node(app) else {
        app.right_state.select(None);
        return;
    };
    let refs = refs_for(app, todo_list, &node);
    app.right_state
        .select((!refs.is_empty()).then_some(refs.len() - 1));
}

fn jump_to_top(app: &mut AppState, todo_list: &TodoList) {
    match app.focus {
        Focus::Left => {
            app.left_state
                .select((!app.tree_items.is_empty()).then_some(0));
            app.right_state.select(Some(0));
        }
        Focus::Right => {
            if let Some(node) = selected_node(app) {
                let refs = refs_for(app, todo_list, &node);
                app.right_state.select((!refs.is_empty()).then_some(0));
            }
        }
    }
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
    app.mode = Mode::InputTask(TaskInput {
        editing_idx: Some(current),
        insert_idx: None,
        above: false,
        node: reference.node,
        field: TextField::new(&task.text),
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
    let refs = refs_for(app, todo_list, &node);
    refs.get(app.right_state.selected().unwrap_or(0)).cloned()
}

fn selected_task_targets(app: &AppState, todo_list: &TodoList, count: usize) -> Vec<TaskRef> {
    let Some(node) = selected_node(app) else {
        return Vec::new();
    };
    let refs = refs_for(app, todo_list, &node);
    let start = app.right_state.selected().unwrap_or(0).min(refs.len());
    let end = start.saturating_add(count).min(refs.len());
    refs[start..end].to_vec()
}

fn yank_task(app: &mut AppState, todo_list: &TodoList) {
    if let Some(reference) = current_task_ref(app, todo_list)
        && let Some(task) = get_task_from_ref(todo_list, &reference)
    {
        app.clipboard = Some(ClipboardItem::Task(task.clone()));
        app.set_info("yanked task");
    }
}

fn yank_section(app: &mut AppState, todo_list: &TodoList) {
    if let Some(node) = selected_node(app)
        && let Some(section) = get_node(todo_list, &node)
    {
        app.clipboard = Some(ClipboardItem::Section(section.clone()));
        app.set_info("yanked section");
    }
}

fn toggle_hide_done(app: &mut AppState) {
    app.hide_done = !app.hide_done;
    app.set_info(if app.hide_done {
        "hiding completed tasks"
    } else {
        "showing completed tasks"
    });
}

fn handle_fold(app: &mut AppState, todo_list: &TodoList, code: KeyCode) {
    match code {
        KeyCode::Char('a') => toggle_fold(app, todo_list),
        KeyCode::Char('M') => {
            let mut paths = HashSet::new();
            collect_foldable(None, &todo_list.sections, &mut paths);
            app.folded = paths;
            app.set_info("folded all sections");
        }
        KeyCode::Char('R') => {
            app.folded.clear();
            app.set_info("unfolded all sections");
        }
        _ => {}
    }
}

fn toggle_fold(app: &mut AppState, todo_list: &TodoList) {
    let Some(node) = selected_node(app) else {
        return;
    };
    let Some(section) = get_node(todo_list, &node) else {
        return;
    };
    if section.children.is_empty() {
        app.set_info("section has no subsections");
        return;
    }
    if !app.folded.remove(&node) {
        app.folded.insert(node);
        app.set_info("section folded");
    } else {
        app.set_info("section unfolded");
    }
}

fn collect_foldable(parent: Option<&NodePath>, children: &[Section], out: &mut HashSet<NodePath>) {
    for (index, child) in children.iter().enumerate() {
        let mut path = parent.map_or_else(Vec::new, Clone::clone);
        path.push(index);
        if !child.children.is_empty() {
            out.insert(path.clone());
            collect_foldable(Some(&path), &child.children, out);
        }
    }
}

fn jump_to_next_overdue(app: &mut AppState, todo_list: &TodoList) {
    let refs = visible_tasks_in_document_order(app, todo_list);
    if refs.is_empty() {
        app.set_info("no overdue tasks");
        return;
    }

    let current = current_task_ref(app, todo_list).and_then(|current| {
        refs.iter().position(|reference| {
            reference.node == current.node && reference.task_idx == current.task_idx
        })
    });
    let start = current.map_or(0, |position| position + 1);
    let found = (0..refs.len())
        .map(|step| (start + step) % refs.len())
        .find(|index| {
            get_task_from_ref(todo_list, &refs[*index])
                .is_some_and(|task| !task.is_done && task.due.is_some_and(|due| due < app.today))
        });
    let Some(index) = found else {
        app.set_info("no overdue tasks");
        return;
    };

    let target = refs[index].clone();
    app.focus = Focus::Right;
    if let Some(row) = app
        .tree_items
        .iter()
        .position(|item| matches!(item, TreeItem::Node(path) if path == &target.node))
    {
        app.left_state.select(Some(row));
    }
    let row = refs_for(app, todo_list, &target.node)
        .iter()
        .position(|reference| {
            reference.node == target.node && reference.task_idx == target.task_idx
        });
    app.right_state.select(row);
    app.set_info("next overdue task");
}

/// Every visible task in document order, including sections other than the
/// selected one.
///
/// `get_task_refs_filtered` already flattens descendants for a top-level
/// section, so each node contributes only the tasks it owns; otherwise a nested
/// task would appear once per ancestor.
fn visible_tasks_in_document_order(app: &AppState, todo_list: &TodoList) -> Vec<TaskRef> {
    let mut refs = Vec::new();
    for item in &app.tree_items {
        let TreeItem::Node(path) = item else {
            continue;
        };
        refs.extend(
            refs_for(app, todo_list, path)
                .into_iter()
                .filter(|reference| reference.node == *path),
        );
    }
    refs
}

fn remember_selection(app: &mut AppState, todo_list: &TodoList) {
    let node = selected_node(app);
    let task = if app.focus == Focus::Right {
        node.as_ref().and_then(|node| {
            let refs = refs_for(app, todo_list, node);
            refs.get(app.right_state.selected().unwrap_or(0)).cloned()
        })
    } else {
        None
    };
    app.filter_restore = Some(SelectionSnapshot { node, task });
    app.filter_anchor = None;
}

/// Handle typing in the live-filter mode.
pub fn handle_filter(app: &mut AppState, todo_list: &TodoList, code: KeyCode) -> Vec<Action> {
    match code {
        KeyCode::Esc => {
            remember_selection(app, todo_list);
            app.clear_filter();
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
    Vec::new()
}

/// Handle keys while the help overlay is open.
pub fn handle_help(app: &mut AppState, code: KeyCode) -> Vec<Action> {
    match code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.mode = Mode::Normal;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.help_scroll = app
                .help_scroll
                .saturating_add(1)
                .min(crate::tui::render::help::max_scroll(app.viewport_height));
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }
        _ => {}
    }
    Vec::new()
}
