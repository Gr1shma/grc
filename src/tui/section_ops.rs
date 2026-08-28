use crate::task::{NodePath, Section, TodoList, get_node, get_node_mut};
use anyhow::Result;

pub fn insert_section(
    todo_list: &mut TodoList,
    parent: Option<&NodePath>,
    idx: usize,
    section: Section,
) -> Result<NodePath> {
    match parent {
        None => {
            let i = idx.min(todo_list.sections.len());
            todo_list.sections.insert(i, section);
            Ok(vec![i])
        }
        Some(p) => {
            let sec = get_node_mut(todo_list, p)
                .ok_or_else(|| anyhow::anyhow!("Parent section not found for path {:?}", p))?;
            let i = idx.min(sec.children.len());
            sec.children.insert(i, section);
            let mut path = p.clone();
            path.push(i);
            Ok(path)
        }
    }
}

pub fn remove_section(todo_list: &mut TodoList, path: &NodePath) -> Option<Section> {
    if path.is_empty() {
        return None;
    }
    if path.len() == 1 {
        if path[0] < todo_list.sections.len() {
            return Some(todo_list.sections.remove(path[0]));
        }
        return None;
    }
    let parent_path = &path[..path.len() - 1];
    let idx = *path.last().unwrap();
    let parent = get_node_mut(todo_list, parent_path)?;
    if idx < parent.children.len() {
        Some(parent.children.remove(idx))
    } else {
        None
    }
}

pub fn child_count(todo_list: &TodoList, path: Option<&NodePath>) -> usize {
    path.map_or(todo_list.sections.len(), |p| {
        get_node(todo_list, p).map_or(0, |s| s.children.len())
    })
}

pub fn node_task_count(todo_list: &TodoList, node: &NodePath) -> usize {
    get_node(todo_list, node).map_or(0, |s| s.tasks.len())
}
