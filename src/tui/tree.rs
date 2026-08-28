use crate::task::{NodePath, Section, Task, TodoList, get_node};
use crate::tui::state::{AppState, Mode, TreeItem};

pub fn build_tree_items(todo_list: &TodoList, mode: &Mode) -> Vec<TreeItem> {
    let ghost = match mode {
        Mode::InputSection {
            node: None,
            parent,
            insert_idx,
            ..
        } => Some((parent.clone(), *insert_idx)),
        _ => None,
    };

    let mut items = Vec::new();
    walk_tree(&mut items, ghost.as_ref(), None, &todo_list.sections, 0);
    items
}

pub fn build_visible_tree_items(
    todo_list: &TodoList,
    mode: &Mode,
    filter_lower: &str,
) -> Vec<TreeItem> {
    if filter_lower.is_empty() || matches!(mode, Mode::InputSection { .. }) {
        return build_tree_items(todo_list, mode);
    }
    // Two-pass filter: prioritize section name matches, fall back to task matches
    let mut name_matched_paths: Vec<NodePath> = Vec::new();
    collect_section_name_matches(&mut name_matched_paths, None, &todo_list.sections, filter_lower);

    let mut items = Vec::new();
    if !name_matched_paths.is_empty() {
        // Some section names matched - show only those sections and their ancestors
        for path in &name_matched_paths {
            // Add all ancestors
            for depth in 0..path.len() {
                let ancestor_path = path[..=depth].to_vec();
                if !items.iter().any(|item| matches!(item, TreeItem::Node(p) if *p == ancestor_path)) {
                    items.push(TreeItem::Node(ancestor_path));
                }
            }
        }
    } else {
        // No section names matched - fall back to task-based filtering
        walk_tree_filtered_by_task(&mut items, None, &todo_list.sections, filter_lower);
    }
    items
}

/// Collect paths of sections whose name matches the filter
fn collect_section_name_matches(
    paths: &mut Vec<NodePath>,
    parent: Option<&NodePath>,
    children: &[Section],
    filter_lower: &str,
) {
    for (i, child) in children.iter().enumerate() {
        let mut p = parent.map_or_else(Vec::new, Clone::clone);
        p.push(i);
        if section_matches(child, filter_lower) {
            paths.push(p.clone());
        }
        collect_section_name_matches(paths, Some(&p), &child.children, filter_lower);
    }
}

/// Fallback: show sections that contain matching tasks
fn walk_tree_filtered_by_task(
    items: &mut Vec<TreeItem>,
    parent: Option<&NodePath>,
    children: &[Section],
    filter_lower: &str,
) {
    for (i, child) in children.iter().enumerate() {
        let mut p = parent.map_or_else(Vec::new, Clone::clone);
        p.push(i);
        let desc_match = subtree_has_matching_task(child, filter_lower);
        if desc_match {
            items.push(TreeItem::Node(p.clone()));
            walk_tree_filtered_by_task(items, Some(&p), &child.children, filter_lower);
        }
    }
}

fn subtree_has_matching_task(sec: &Section, filter_lower: &str) -> bool {
    if sec.tasks.iter().any(|t| task_matches(t, filter_lower)) {
        return true;
    }
    sec.children.iter().any(|c| subtree_has_matching_task(c, filter_lower))
}

pub(crate) fn section_matches(sec: &Section, filter_lower: &str) -> bool {
    sec.name.to_lowercase().contains(filter_lower)
}

pub(crate) fn task_matches(task: &Task, filter_lower: &str) -> bool {
    if task.text.to_lowercase().contains(filter_lower) {
        return true;
    }
    task.due.is_some_and(|d| {
        d.format("%Y-%m-%d").to_string().to_lowercase().contains(filter_lower)
    })
}

fn walk_tree(
    items: &mut Vec<TreeItem>,
    ghost: Option<&(Option<NodePath>, Option<usize>)>,
    parent: Option<&NodePath>,
    children: &[Section],
    depth: usize,
) {
    let n = children.len();
    for (i, child) in children.iter().enumerate() {
        if let Some((gp, gi)) = ghost
            && gp.as_deref() == parent.map(Vec::as_slice)
            && *gi == Some(i)
        {
            items.push(TreeItem::Ghost(depth));
        }
        let mut p = parent.map_or_else(Vec::new, Clone::clone);
        p.push(i);
        items.push(TreeItem::Node(p.clone()));
        walk_tree(items, ghost, Some(&p), &child.children, depth + 1);
    }
    if let Some((gp, gi)) = ghost
        && gp.as_deref() == parent.map(Vec::as_slice)
        && (gi.is_none() || *gi == Some(n))
    {
        items.push(TreeItem::Ghost(depth));
    }
}

pub fn rebuild_and_select(app: &mut AppState, todo_list: &TodoList, path: &NodePath) {
    app.tree_items = build_tree_items(todo_list, &app.mode);
    if let Some(pos) = app
        .tree_items
        .iter()
        .position(|i| matches!(i, TreeItem::Node(p) if p == path))
    {
        app.left_state.select(Some(pos));
    }
}

pub fn selected_node(app: &AppState) -> Option<NodePath> {
    let idx = app.left_state.selected()?;
    match app.tree_items.get(idx)? {
        TreeItem::Node(path) => Some(path.clone()),
        TreeItem::Ghost(_) => None,
    }
}

pub fn node_name(todo_list: &TodoList, node: &NodePath) -> String {
    get_node(todo_list, node).map_or_else(String::new, |s| s.name.clone())
}
