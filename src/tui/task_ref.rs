//! Mapping task-list rows back to tasks in the section tree.
//!
//! A top-level section shows every task in its subtree, so each row needs the
//! exact node path and index within that node. Nested sections show only their
//! direct tasks.

use crate::task::{NodePath, Task, TodoList, get_node, get_node_mut};
use crate::tui::tree::{section_matches, task_matches};

/// A task plus its location in the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskRef {
    /// Section that directly owns the task.
    pub node: NodePath,
    /// Index within `node.tasks`.
    pub task_idx: usize,
    /// Breadcrumb for tasks pulled in from descendant sections.
    pub sub_name: Option<String>,
}

/// Rows shown when `node` is selected.
pub fn get_task_refs(todo_list: &TodoList, node: &NodePath) -> Vec<TaskRef> {
    let Some(section) = get_node(todo_list, node) else {
        return Vec::new();
    };

    if node.len() > 1 {
        return section
            .tasks
            .iter()
            .enumerate()
            .map(|(task_idx, _)| TaskRef {
                node: node.clone(),
                task_idx,
                sub_name: None,
            })
            .collect();
    }

    let mut refs = Vec::new();
    collect_subtree_refs(todo_list, node, &mut refs);
    refs
}

/// Apply the active filter to the task rows.
///
/// A matching section name keeps all of its tasks. Otherwise tasks match by
/// text, due date, or the breadcrumb of the subsection they came from.
pub fn get_task_refs_filtered(
    todo_list: &TodoList,
    node: &NodePath,
    filter_lower: &str,
) -> Vec<TaskRef> {
    let refs = get_task_refs(todo_list, node);
    if filter_lower.is_empty() {
        return refs;
    }
    if let Some(section) = get_node(todo_list, node)
        && section_matches(section, filter_lower)
    {
        return refs;
    }

    refs.into_iter()
        .filter(|task_ref| {
            get_task_from_ref(todo_list, task_ref)
                .is_some_and(|task| task_matches(task, filter_lower))
                || task_ref
                    .sub_name
                    .as_deref()
                    .is_some_and(|name| name.to_lowercase().contains(filter_lower))
        })
        .collect()
}

fn collect_subtree_refs(todo_list: &TodoList, node: &NodePath, out: &mut Vec<TaskRef>) {
    let Some(section) = get_node(todo_list, node) else {
        return;
    };

    let breadcrumb = node_breadcrumb(todo_list, node);
    for task_idx in 0..section.tasks.len() {
        out.push(TaskRef {
            node: node.clone(),
            task_idx,
            sub_name: breadcrumb.clone(),
        });
    }
    for child_idx in 0..section.children.len() {
        let mut child = node.clone();
        child.push(child_idx);
        collect_subtree_refs(todo_list, &child, out);
    }
}

/// Breadcrumb of descendant names, excluding the selected top-level section.
pub fn node_breadcrumb(todo_list: &TodoList, node: &NodePath) -> Option<String> {
    if node.len() <= 1 {
        return None;
    }
    let parts: Vec<String> = (1..node.len())
        .filter_map(|depth| get_node(todo_list, &node[..=depth]))
        .map(|section| section.name.clone())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" › "))
}

/// Full breadcrumb including the selected section.
pub fn full_breadcrumb(todo_list: &TodoList, path: &NodePath) -> String {
    let parts: Vec<String> = (0..path.len())
        .filter_map(|depth| get_node(todo_list, &path[..=depth]))
        .map(|section| section.name.clone())
        .collect();
    parts.join(" › ")
}

/// Borrow a task through its reference.
pub fn get_task_from_ref<'a>(todo_list: &'a TodoList, task_ref: &TaskRef) -> Option<&'a Task> {
    get_node(todo_list, &task_ref.node)?
        .tasks
        .get(task_ref.task_idx)
}

/// Mutably borrow a task through its reference.
pub fn get_task_from_ref_mut<'a>(
    todo_list: &'a mut TodoList,
    task_ref: &TaskRef,
) -> Option<&'a mut Task> {
    get_node_mut(todo_list, &task_ref.node)?
        .tasks
        .get_mut(task_ref.task_idx)
}
