use crate::task::{NodePath, Task, TodoList, get_node, get_node_mut};
use crate::tui::tree::{section_matches, task_matches};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskRef {
    pub node: NodePath,
    pub task_idx: usize,
    pub sub_name: Option<String>,
}

pub fn get_task_refs(todo_list: &TodoList, node: &NodePath) -> Vec<TaskRef> {
    get_node(todo_list, node).map_or_else(Vec::new, |sec| {
        if node.len() == 1 {
            let mut out = Vec::new();
            collect_flatten(todo_list, node, &mut out);
            out
        } else {
            sec.tasks
                .iter()
                .enumerate()
                .map(|(i, _)| TaskRef {
                    node: node.clone(),
                    task_idx: i,
                    sub_name: None,
                })
                .collect()
        }
    })
}

pub fn get_task_refs_filtered(
    todo_list: &TodoList,
    node: &NodePath,
    filter_lower: &str,
) -> Vec<TaskRef> {
    let refs = get_task_refs(todo_list, node);
    if filter_lower.is_empty() {
        return refs;
    }
    if let Some(sec) = get_node(todo_list, node)
        && section_matches(sec, filter_lower)
    {
        return refs;
    }
    refs.into_iter()
        .filter(|r| {
            get_task_from_ref(todo_list, r)
                .map(|task| task_matches(task, filter_lower))
                .unwrap_or(false)
                || r.sub_name
                    .as_deref()
                    .is_some_and(|s| s.to_lowercase().contains(filter_lower))
        })
        .collect()
}

fn collect_flatten(todo_list: &TodoList, node: &NodePath, out: &mut Vec<TaskRef>) {
    if let Some(sec) = get_node(todo_list, node) {
        let sub_name = node_breadcrumb(todo_list, node);
        for (i, _) in sec.tasks.iter().enumerate() {
            out.push(TaskRef {
                node: node.clone(),
                task_idx: i,
                sub_name: sub_name.clone(),
            });
        }
        let child_count = sec.children.len();
        for ci in 0..child_count {
            let mut cp = node.clone();
            cp.push(ci);
            collect_flatten(todo_list, &cp, out);
        }
    }
}

pub fn node_breadcrumb(todo_list: &TodoList, node: &NodePath) -> Option<String> {
    if node.len() <= 1 {
        return None;
    }
    let parts: Vec<String> = (1..node.len())
        .filter_map(|k| get_node(todo_list, &node[..=k]))
        .map(|sec| sec.name.clone())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" › "))
    }
}

pub fn full_breadcrumb(todo_list: &TodoList, path: &NodePath) -> String {
    let parts: Vec<String> = (0..path.len())
        .filter_map(|k| get_node(todo_list, &path[..=k]))
        .map(|sec| sec.name.clone())
        .collect();
    parts.join(" › ")
}

pub fn get_task_from_ref<'a>(todo_list: &'a TodoList, ref_item: &TaskRef) -> Option<&'a Task> {
    let sec = get_node(todo_list, &ref_item.node)?;
    sec.tasks.get(ref_item.task_idx)
}

pub fn get_task_from_ref_mut<'a>(
    todo_list: &'a mut TodoList,
    ref_item: &TaskRef,
) -> Option<&'a mut Task> {
    let sec = get_node_mut(todo_list, &ref_item.node)?;
    sec.tasks.get_mut(ref_item.task_idx)
}
