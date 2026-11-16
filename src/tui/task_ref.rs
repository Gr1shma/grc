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
    hide_done: bool,
) -> Vec<TaskRef> {
    let mut refs = get_task_refs(todo_list, node);
    if hide_done {
        refs.retain(|task_ref| {
            get_task_from_ref(todo_list, task_ref).is_some_and(|task| !task.is_done)
        });
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_str;

    fn sample() -> TodoList {
        parse_str(
            "# work\n- [ ] Task A\n- [ ] Task B\n## backend\n- [ ] Fix API\n### api\n- [ ] Fix endpoint\n# personal\n- [ ] Buy milk\n",
        )
    }

    #[test]
    fn top_level_flattens_subtree() {
        let refs = get_task_refs(&sample(), &vec![0]);
        assert_eq!(refs.len(), 4);
        assert_eq!(refs[0].node, vec![0]);
        assert_eq!(refs[0].sub_name, None);
        assert_eq!(refs[2].node, vec![0, 0]);
        assert_eq!(refs[2].sub_name.as_deref(), Some("backend"));
        assert_eq!(refs[3].node, vec![0, 0, 0]);
        assert_eq!(refs[3].sub_name.as_deref(), Some("backend › api"));
    }

    #[test]
    fn nested_section_shows_direct_tasks_only() {
        let refs = get_task_refs(&sample(), &vec![0, 0]);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].node, vec![0, 0]);
        assert_eq!(refs[0].sub_name, None);
    }

    #[test]
    fn missing_section_has_no_rows() {
        assert!(get_task_refs(&sample(), &vec![99]).is_empty());
        assert!(get_task_refs(&TodoList::default(), &vec![0]).is_empty());
    }

    #[test]
    fn filter_by_task_text() {
        let refs = get_task_refs_filtered(&sample(), &vec![0], "milk", false);
        assert!(refs.is_empty());
        let refs = get_task_refs_filtered(&sample(), &vec![1], "milk", false);
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn filter_by_breadcrumb() {
        let refs = get_task_refs_filtered(&sample(), &vec![0], "backend", false);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].node, vec![0, 0]);
        assert_eq!(refs[1].node, vec![0, 0, 0]);
    }

    #[test]
    fn matching_section_keeps_all_tasks() {
        let refs = get_task_refs_filtered(&sample(), &vec![0], "work", false);
        assert_eq!(refs.len(), 4);
        let refs = get_task_refs_filtered(&sample(), &vec![0, 0], "backend", false);
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn task_accessors() {
        let mut list = sample();
        let task_ref = TaskRef {
            node: vec![0, 0, 0],
            task_idx: 0,
            sub_name: Some("api".into()),
        };
        assert_eq!(
            get_task_from_ref(&list, &task_ref).unwrap().text,
            "Fix endpoint"
        );

        get_task_from_ref_mut(&mut list, &task_ref).unwrap().text = "Updated".into();
        assert_eq!(
            get_node(&list, &[0, 0, 0]).unwrap().tasks[0].text,
            "Updated"
        );
    }

    #[test]
    fn breadcrumbs() {
        let list = sample();
        assert_eq!(node_breadcrumb(&list, &vec![0]), None);
        assert_eq!(
            node_breadcrumb(&list, &vec![0, 0, 0]).as_deref(),
            Some("backend › api")
        );
        assert_eq!(
            full_breadcrumb(&list, &vec![0, 0, 0]),
            "work › backend › api"
        );
    }
}
