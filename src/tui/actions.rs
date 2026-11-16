//! Document-mutating actions and the single place that applies them.
//!
//! Input handlers only decide *what* should happen and return an [`Action`].
//! This module applies actions to the document and updates selection state.
//! Persistence and undo history live one level up in [`super::engine`].

use crate::task::{
    NodePath, Section, Task, TodoList, child_count, get_node_mut, insert_section, remove_section,
    task_count,
};
use crate::tui::state::{AppState, ClipboardItem, Focus, Mode, SectionInput, TaskInput};
use crate::tui::task_ref::{TaskRef, get_task_from_ref_mut, get_task_refs_filtered};
use crate::tui::tree::{
    build_visible_tree_items, clamp_left_selection, clamp_right_selection, node_level,
    selected_node,
};
use chrono::NaiveDate;

/// A requested document or application operation.
#[derive(Clone, Debug)]
pub enum Action {
    /// Commit a new or edited task from an input field.
    CommitTask {
        /// Original input target.
        input: TaskInput,
        /// Cleaned task text.
        text: String,
        /// Due date already extracted from the text.
        due: Option<NaiveDate>,
    },
    /// Set or clear one task's due date.
    SetDue {
        /// Task to edit.
        target: TaskRef,
        /// New due date; `None` clears it.
        due: Option<NaiveDate>,
    },
    /// Toggle one task's checkbox.
    ToggleTask {
        /// Task to toggle.
        target: TaskRef,
    },
    /// Delete one or more visible tasks.
    DeleteTasks {
        /// Tasks selected from the current filtered row list.
        targets: Vec<TaskRef>,
    },
    /// Delete one section and its subtree.
    DeleteSection {
        /// Section path.
        path: NodePath,
    },
    /// Paste the clipboard item relative to the current selection.
    Paste {
        /// Paste above the anchor rather than below it.
        above: bool,
    },
    /// Recursively sort a section subtree by due date.
    Sort {
        /// Subtree root.
        node: NodePath,
    },
    /// Commit a new or renamed section.
    CommitSection {
        /// Original input target.
        input: SectionInput,
        /// Trimmed section name.
        name: String,
    },
    /// Move a task among its direct siblings.
    MoveTask {
        /// Task to move.
        target: TaskRef,
        /// `-1` for up, `1` for down.
        delta: isize,
    },
    /// Move a section among its siblings.
    MoveSection {
        /// Section path.
        path: NodePath,
        /// `-1` for up, `1` for down.
        delta: isize,
    },
    /// Undo the last document mutation.
    Undo,
    /// Redo the last undone mutation.
    Redo,
    /// Reload the file after a conflict.
    Reload,
    /// Overwrite the file after a conflict.
    Overwrite,
    /// Save and exit.
    Quit,
}

/// Apply one action.
///
/// Returns `true` when the document changed and needs saving.
pub fn apply(action: Action, app: &mut AppState, todo_list: &mut TodoList) -> bool {
    match action {
        Action::CommitTask { input, text, due } => commit_task(app, todo_list, &input, text, due),
        Action::SetDue { target, due } => set_due(app, todo_list, &target, due),
        Action::ToggleTask { target } => toggle_task(app, todo_list, &target),
        Action::DeleteTasks { targets } => delete_tasks(app, todo_list, targets),
        Action::DeleteSection { path } => delete_section(app, todo_list, &path),
        Action::Paste { above } => paste(app, todo_list, above),
        Action::Sort { node } => sort(app, todo_list, &node),
        Action::CommitSection { input, name } => commit_section(app, todo_list, &input, &name),
        Action::MoveTask { target, delta } => move_task(app, todo_list, &target, delta),
        Action::MoveSection { path, delta } => move_section(app, todo_list, &path, delta),
        Action::Undo | Action::Redo | Action::Reload | Action::Overwrite | Action::Quit => false,
    }
}

fn refs_for(app: &AppState, todo_list: &TodoList, node: &NodePath) -> Vec<TaskRef> {
    get_task_refs_filtered(todo_list, node, &app.filter_lower, app.hide_done)
}

fn rebuild_tree(app: &mut AppState, todo_list: &TodoList) {
    app.tree_items = build_visible_tree_items(
        todo_list,
        &app.mode,
        &app.filter_lower,
        &app.folded,
        app.hide_done,
    );
    clamp_left_selection(app);
}

fn select_path(app: &mut AppState, path: &[usize]) -> bool {
    if let Some(position) = app.tree_items.iter().position(
        |item| matches!(item, crate::tui::state::TreeItem::Node(p) if p.as_slice() == path),
    ) {
        app.left_state.select(Some(position));
        true
    } else {
        false
    }
}

fn commit_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    input: &TaskInput,
    text: String,
    due: Option<NaiveDate>,
) -> bool {
    match input.editing_idx {
        None => insert_task(app, todo_list, input, text, due),
        Some(editing_idx) => rename_task(app, todo_list, input, editing_idx, text, due),
    }
}

fn insert_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    input: &TaskInput,
    text: String,
    due: Option<NaiveDate>,
) -> bool {
    let refs = refs_for(app, todo_list, &input.node);
    let anchor = input.insert_idx.map_or(refs.len(), |position| {
        if input.above {
            position
        } else {
            position.saturating_sub(1)
        }
    });
    let (target_node, insert_index) = refs.get(anchor).map_or_else(
        || (input.node.clone(), task_count(todo_list, &input.node)),
        |reference| {
            let base = reference.task_idx;
            (
                reference.node.clone(),
                if input.above { base } else { base + 1 },
            )
        },
    );

    let Some(section) = get_node_mut(todo_list, &target_node) else {
        return false;
    };
    let mut task = Task::new(text);
    task.due = due;
    section.tasks.insert(insert_index, task);

    app.focus = Focus::Right;
    app.mode = Mode::Normal;
    let new_refs = refs_for(app, todo_list, &target_node);
    if let Some(position) = new_refs.iter().position(|reference| {
        reference.node.as_slice() == target_node.as_slice() && reference.task_idx == insert_index
    }) {
        app.right_state.select(Some(position));
    } else if new_refs.is_empty() {
        app.right_state.select(None);
    } else {
        let row = input
            .insert_idx
            .unwrap_or(new_refs.len())
            .min(new_refs.len() - 1);
        app.right_state.select(Some(row));
    }
    rebuild_tree(app, todo_list);
    app.set_success("task added");
    true
}

fn rename_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    input: &TaskInput,
    editing_idx: usize,
    text: String,
    due: Option<NaiveDate>,
) -> bool {
    let refs = refs_for(app, todo_list, &input.node);
    let Some(reference) = refs.get(editing_idx) else {
        return false;
    };
    let Some(task) = get_task_from_ref_mut(todo_list, reference) else {
        return false;
    };
    let changed = task.text != text || (due.is_some() && task.due != due);
    task.text = text;
    if due.is_some() {
        task.due = due;
    }
    app.mode = Mode::Normal;
    if changed {
        app.set_success("task updated");
    }
    changed
}

fn set_due(
    app: &mut AppState,
    todo_list: &mut TodoList,
    target: &TaskRef,
    due: Option<NaiveDate>,
) -> bool {
    let Some(task) = get_task_from_ref_mut(todo_list, target) else {
        return false;
    };
    if task.due == due {
        return false;
    }
    task.due = due;
    app.mode = Mode::Normal;
    app.set_success(if due.is_some() {
        "due date set"
    } else {
        "due date cleared"
    });
    true
}

fn toggle_task(app: &mut AppState, todo_list: &mut TodoList, target: &TaskRef) -> bool {
    let Some(task) = get_task_from_ref_mut(todo_list, target) else {
        return false;
    };
    task.is_done = !task.is_done;
    app.set_success(if task.is_done {
        "task completed"
    } else {
        "task reopened"
    });
    true
}

fn delete_tasks(app: &mut AppState, todo_list: &mut TodoList, mut targets: Vec<TaskRef>) -> bool {
    if targets.is_empty() {
        return false;
    }
    targets.sort_by(|a, b| {
        b.node
            .cmp(&a.node)
            .then_with(|| b.task_idx.cmp(&a.task_idx))
    });
    targets.dedup();

    let mut removed = Vec::new();
    for target in &targets {
        if let Some(section) = get_node_mut(todo_list, &target.node)
            && target.task_idx < section.tasks.len()
        {
            removed.push(section.tasks.remove(target.task_idx));
        }
    }
    if removed.is_empty() {
        return false;
    }
    app.clipboard = removed.pop().map(ClipboardItem::Task);

    rebuild_tree(app, todo_list);
    let selected_node = selected_node(app);
    let row_count = selected_node
        .as_ref()
        .map_or(0, |node| refs_for(app, todo_list, node).len());
    if row_count == 0 {
        app.right_state.select(None);
        app.focus = Focus::Left;
    } else {
        let current = app.right_state.selected().unwrap_or(0);
        app.right_state.select(Some(current.min(row_count - 1)));
    }
    app.set_success("task deleted");
    true
}

fn delete_section(app: &mut AppState, todo_list: &mut TodoList, path: &[usize]) -> bool {
    let old_row = app.left_state.selected().unwrap_or(0);
    let Some(removed) = remove_section(todo_list, path) else {
        return false;
    };
    app.clipboard = Some(ClipboardItem::Section(removed));
    rebuild_tree(app, todo_list);
    if app.tree_items.is_empty() {
        app.left_state.select(None);
    } else {
        app.left_state.select(Some(
            old_row.saturating_sub(1).min(app.tree_items.len() - 1),
        ));
    }
    app.right_state.select(None);
    app.filter_anchor = None;
    app.set_success("section deleted");
    true
}

fn paste(app: &mut AppState, todo_list: &mut TodoList, above: bool) -> bool {
    let Some(clipboard) = app.clipboard.clone() else {
        return false;
    };
    match clipboard {
        ClipboardItem::Task(task) => paste_task(app, todo_list, task, above),
        ClipboardItem::Section(section) => paste_section(app, todo_list, section, above),
    }
}

fn paste_task(app: &mut AppState, todo_list: &mut TodoList, task: Task, above: bool) -> bool {
    let Some(selected) = selected_node(app) else {
        return false;
    };
    let refs = refs_for(app, todo_list, &selected);
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
        return false;
    };
    let insert_index = insert_index.min(section.tasks.len());
    section.tasks.insert(insert_index, task);
    rebuild_tree(app, todo_list);

    if app.focus == Focus::Right {
        let new_refs = refs_for(app, todo_list, &selected);
        if let Some(position) = new_refs.iter().position(|reference| {
            reference.node.as_slice() == target_node.as_slice()
                && reference.task_idx == insert_index
        }) {
            app.right_state.select(Some(position));
        } else {
            clamp_right_selection(app, new_refs.len());
        }
    }
    app.set_success("pasted");
    true
}

fn paste_section(
    app: &mut AppState,
    todo_list: &mut TodoList,
    section: Section,
    above: bool,
) -> bool {
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
        return false;
    };
    app.mode = Mode::Normal;
    rebuild_tree(app, todo_list);
    select_path(app, &new_path);
    app.right_state.select(None);
    app.set_success("pasted");
    true
}

fn sort(app: &mut AppState, todo_list: &mut TodoList, node: &NodePath) -> bool {
    let Some(section) = get_node_mut(todo_list, node) else {
        return false;
    };
    let changed = sort_section(section);
    if changed {
        app.set_success("sorted by due date");
    } else {
        app.set_info("already sorted");
    }
    changed
}

fn sort_section(section: &mut Section) -> bool {
    let before = section.tasks.clone();
    section
        .tasks
        .sort_by_key(|task| (task.is_done, task.due.is_none(), task.due));
    let mut changed = before != section.tasks;
    for child in &mut section.children {
        changed |= sort_section(child);
    }
    changed
}

fn commit_section(
    app: &mut AppState,
    todo_list: &mut TodoList,
    input: &SectionInput,
    name: &str,
) -> bool {
    match &input.node {
        None => {
            let section = Section::with_level(name, input.level);
            let Some(new_path) = insert_section(
                todo_list,
                input.parent.as_ref(),
                input.insert_idx.unwrap_or(usize::MAX),
                section,
                input.level,
            ) else {
                return false;
            };
            app.mode = Mode::Normal;
            app.folded.remove(&new_path);
            rebuild_tree(app, todo_list);
            select_path(app, &new_path);
            app.right_state.select(None);
            app.set_success("section added");
            true
        }
        Some(existing) => {
            let Some(section) = get_node_mut(todo_list, existing) else {
                return false;
            };
            let changed = section.name != name;
            section.name = name.to_string();
            app.mode = Mode::Normal;
            app.folded.remove(existing);
            rebuild_tree(app, todo_list);
            select_path(app, existing);
            if changed {
                app.set_success("section renamed");
            }
            changed
        }
    }
}

fn move_task(app: &mut AppState, todo_list: &mut TodoList, target: &TaskRef, delta: isize) -> bool {
    let visible_node = selected_node(app);
    let refs = visible_node
        .as_ref()
        .map_or_else(Vec::new, |node| refs_for(app, todo_list, node));
    let Some(position) = refs.iter().position(|reference| {
        reference.node == target.node && reference.task_idx == target.task_idx
    }) else {
        return false;
    };
    // Reorder against the neighbour the user can see. Swapping with the next
    // document index would silently move the task past hidden or filtered rows.
    let Some(neighbour) =
        offset_index(position, refs.len(), delta).and_then(|index| refs.get(index))
    else {
        return false;
    };
    // A swap can only reorder siblings; moving between sections is not a move.
    if neighbour.node != target.node {
        return false;
    }
    let new_index = neighbour.task_idx;

    let Some(section) = get_node_mut(todo_list, &target.node) else {
        return false;
    };
    if target.task_idx >= section.tasks.len() || new_index >= section.tasks.len() {
        return false;
    }
    section.tasks.swap(target.task_idx, new_index);

    if let Some(position) = refs
        .iter()
        .position(|reference| reference.node == target.node && reference.task_idx == new_index)
    {
        app.focus = Focus::Right;
        app.right_state.select(Some(position));
    } else {
        clamp_right_selection(app, refs.len());
    }
    app.set_success(if delta < 0 {
        "task moved up"
    } else {
        "task moved down"
    });
    true
}

fn move_section(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &NodePath,
    delta: isize,
) -> bool {
    let Some((last, parent_path)) = path.split_last() else {
        return false;
    };
    let siblings = if parent_path.is_empty() {
        &mut todo_list.sections
    } else {
        let Some(parent) = get_node_mut(todo_list, parent_path) else {
            return false;
        };
        &mut parent.children
    };
    let Some(new_index) = offset_index(*last, siblings.len(), delta) else {
        return false;
    };
    siblings.swap(*last, new_index);

    let mut new_path = parent_path.to_vec();
    new_path.push(new_index);
    rebuild_tree(app, todo_list);
    select_path(app, &new_path);
    app.set_success(if delta < 0 {
        "section moved up"
    } else {
        "section moved down"
    });
    true
}

fn offset_index(index: usize, len: usize, delta: isize) -> Option<usize> {
    let new_index = index as isize + delta;
    (new_index >= 0 && (new_index as usize) < len).then_some(new_index as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_str;
    use crate::tui::state::TextField;
    use crate::tui::tree::{build_tree_items, selected_node};

    fn app(list: &TodoList) -> AppState {
        let today = NaiveDate::from_ymd_opt(2025, 7, 21).unwrap();
        let mut app = AppState::new_with_today(list.sections.len(), today);
        app.tree_items = build_tree_items(list, &app.mode, &app.folded);
        app
    }

    #[test]
    fn recursive_sort_orders_undone_then_due_and_is_stable() {
        let mut list = parse_str(
            "# main\n- [x] done early due:2025-01-01\n- [ ] no date\n- [ ] later due:2025-12-01\n- [ ] same due:2025-06-01\n- [ ] earlier due:2025-06-01\n## child\n- [ ] child due:2025-02-01\n- [x] child done\n",
        );
        let mut app = app(&list);
        let changed = apply(Action::Sort { node: vec![0] }, &mut app, &mut list);
        assert!(changed);

        let texts: Vec<_> = list.sections[0]
            .tasks
            .iter()
            .map(|task| task.text.as_str())
            .collect();
        assert_eq!(texts, ["same", "earlier", "later", "no date", "done early"]);
        let dues: Vec<_> = list.sections[0].tasks.iter().map(|task| task.due).collect();
        assert_eq!(
            dues,
            [
                NaiveDate::from_ymd_opt(2025, 6, 1),
                NaiveDate::from_ymd_opt(2025, 6, 1),
                NaiveDate::from_ymd_opt(2025, 12, 1),
                None,
                NaiveDate::from_ymd_opt(2025, 1, 1),
            ]
        );
        let child_texts: Vec<_> = list.sections[0].children[0]
            .tasks
            .iter()
            .map(|task| task.text.as_str())
            .collect();
        assert_eq!(child_texts, ["child", "child done"]);
    }

    #[test]
    fn delete_multiple_tasks_uses_visible_order() {
        let mut list = parse_str("# main\n- [ ] A\n- [ ] B\n- [ ] C\n- [ ] D\n");
        let mut app = app(&list);
        app.focus = Focus::Right;
        app.right_state.select(Some(1));
        let refs = refs_for(&app, &list, &vec![0]);
        let targets = refs[1..3].to_vec();
        let changed = apply(Action::DeleteTasks { targets }, &mut app, &mut list);
        assert!(changed);
        let texts: Vec<_> = list.sections[0]
            .tasks
            .iter()
            .map(|task| task.text.as_str())
            .collect();
        assert_eq!(texts, ["A", "D"]);
        assert!(matches!(app.clipboard, Some(ClipboardItem::Task(_))));
    }

    #[test]
    fn moving_tasks_and_sections_updates_selection() {
        let mut list = parse_str("# a\n- [ ] A\n- [ ] B\n# b\n# c\n");
        let mut app = app(&list);
        let target = TaskRef {
            node: vec![0],
            task_idx: 1,
            sub_name: None,
        };
        assert!(apply(
            Action::MoveTask { target, delta: -1 },
            &mut app,
            &mut list
        ));
        assert_eq!(list.sections[0].tasks[0].text, "B");
        assert_eq!(app.right_state.selected(), Some(0));

        app.left_state.select(Some(2));
        assert!(apply(
            Action::MoveSection {
                path: vec![2],
                delta: -1
            },
            &mut app,
            &mut list
        ));
        assert_eq!(list.sections[1].name, "c");
        assert_eq!(selected_node(&app), Some(vec![1]));
    }

    #[test]
    fn moving_a_task_reorders_against_visible_rows_only() {
        let mut list = parse_str("# main\n- [ ] A\n- [x] B\n- [ ] C\n");
        let mut app = app(&list);
        app.hide_done = true;
        app.focus = Focus::Right;
        let target = TaskRef {
            node: vec![0],
            task_idx: 0,
            sub_name: None,
        };
        assert!(apply(
            Action::MoveTask { target, delta: 1 },
            &mut app,
            &mut list
        ));

        let texts: Vec<_> = list.sections[0]
            .tasks
            .iter()
            .map(|task| task.text.as_str())
            .collect();
        // C and A swap; the hidden done task stays where it was.
        assert_eq!(texts, ["C", "B", "A"]);
        assert_eq!(app.right_state.selected(), Some(1));
    }

    #[test]
    fn moving_a_task_never_crosses_into_a_child_section() {
        let mut list = parse_str("# main\n- [ ] A\n## child\n- [ ] B\n");
        let mut app = app(&list);
        app.focus = Focus::Right;
        let target = TaskRef {
            node: vec![0],
            task_idx: 0,
            sub_name: None,
        };
        assert!(!apply(
            Action::MoveTask { target, delta: 1 },
            &mut app,
            &mut list
        ));
        assert_eq!(list.sections[0].tasks[0].text, "A");
        assert_eq!(list.sections[0].children[0].tasks[0].text, "B");
    }

    #[test]
    fn setting_the_same_due_date_is_a_noop() {
        let mut list = parse_str("# main\n- [ ] A due:2025-06-15\n");
        let mut app = app(&list);
        let target = TaskRef {
            node: vec![0],
            task_idx: 0,
            sub_name: None,
        };
        let due = NaiveDate::from_ymd_opt(2025, 6, 15);
        assert!(!apply(Action::SetDue { target, due }, &mut app, &mut list));
    }

    #[test]
    fn section_commit_and_rename_use_the_requested_target() {
        let mut list = parse_str("# main\n");
        let mut app = app(&list);
        let input = SectionInput {
            node: None,
            parent: Some(vec![0]),
            insert_idx: Some(0),
            level: 2,
            field: TextField::new("child"),
        };
        assert!(apply(
            Action::CommitSection {
                input,
                name: "child".into()
            },
            &mut app,
            &mut list
        ));
        assert_eq!(list.sections[0].children[0].name, "child");
        assert_eq!(selected_node(&app), Some(vec![0, 0]));
    }
}
