//! Flattening the section tree for display and navigation.
//!
//! The renderer and the input handlers both work with the same flattened list
//! produced here. Filtering never mutates the document; it only decides which
//! section rows are visible.

use crate::task::{NodePath, Section, Task, TodoList, get_node};
use crate::tui::state::{AppState, Mode, TreeItem};

/// Flatten every section into depth-first rows.
pub fn build_tree_items(todo_list: &TodoList, mode: &Mode) -> Vec<TreeItem> {
    let ghost = match mode {
        Mode::InputSection(input) if input.node.is_none() => {
            Some((input.parent.clone(), input.insert_idx))
        }
        _ => None,
    };

    let mut items = Vec::new();
    walk_tree(&mut items, ghost.as_ref(), None, &todo_list.sections, 0);
    items
}

/// Flatten sections for the active filter.
///
/// Section-name matches take priority; if no section name matches, sections
/// that contain matching tasks remain visible. Section input bypasses filtering
/// so the ghost row stays reachable.
pub fn build_visible_tree_items(
    todo_list: &TodoList,
    mode: &Mode,
    filter_lower: &str,
) -> Vec<TreeItem> {
    if filter_lower.is_empty() || mode.is_section_input() {
        return build_tree_items(todo_list, mode);
    }

    let mut name_matched_paths = Vec::new();
    collect_section_name_matches(
        &mut name_matched_paths,
        None,
        &todo_list.sections,
        filter_lower,
    );

    if name_matched_paths.is_empty() {
        let mut items = Vec::new();
        walk_tree_filtered_by_task(&mut items, None, &todo_list.sections, filter_lower);
        return items;
    }

    let mut items: Vec<TreeItem> = Vec::new();
    for path in &name_matched_paths {
        for depth in 0..path.len() {
            let ancestor = path[..=depth].to_vec();
            if !items
                .iter()
                .any(|item| matches!(item, TreeItem::Node(p) if *p == ancestor))
            {
                items.push(TreeItem::Node(ancestor));
            }
        }
    }
    items
}

fn collect_section_name_matches(
    paths: &mut Vec<NodePath>,
    parent: Option<&NodePath>,
    children: &[Section],
    filter_lower: &str,
) {
    for (index, child) in children.iter().enumerate() {
        let mut path = parent.map_or_else(Vec::new, Clone::clone);
        path.push(index);
        if section_matches(child, filter_lower) {
            paths.push(path.clone());
        }
        collect_section_name_matches(paths, Some(&path), &child.children, filter_lower);
    }
}

fn walk_tree_filtered_by_task(
    items: &mut Vec<TreeItem>,
    parent: Option<&NodePath>,
    children: &[Section],
    filter_lower: &str,
) {
    for (index, child) in children.iter().enumerate() {
        let mut path = parent.map_or_else(Vec::new, Clone::clone);
        path.push(index);
        if subtree_has_matching_task(child, filter_lower) {
            items.push(TreeItem::Node(path.clone()));
            walk_tree_filtered_by_task(items, Some(&path), &child.children, filter_lower);
        }
    }
}

fn subtree_has_matching_task(section: &Section, filter_lower: &str) -> bool {
    section
        .tasks
        .iter()
        .any(|task| task_matches(task, filter_lower))
        || section
            .children
            .iter()
            .any(|child| subtree_has_matching_task(child, filter_lower))
}

/// Whether a section name contains the (lowercase) filter.
pub(crate) fn section_matches(section: &Section, filter_lower: &str) -> bool {
    section.name.to_lowercase().contains(filter_lower)
}

/// Whether task text or due date contains the (lowercase) filter.
pub(crate) fn task_matches(task: &Task, filter_lower: &str) -> bool {
    task.text.to_lowercase().contains(filter_lower)
        || task.due.is_some_and(|due| {
            due.format("%Y-%m-%d")
                .to_string()
                .to_lowercase()
                .contains(filter_lower)
        })
}

fn walk_tree(
    items: &mut Vec<TreeItem>,
    ghost: Option<&(Option<NodePath>, Option<usize>)>,
    parent: Option<&NodePath>,
    children: &[Section],
    depth: usize,
) {
    for (index, child) in children.iter().enumerate() {
        if ghost.is_some_and(|(ghost_parent, insert_at)| {
            ghost_parent.as_deref() == parent.map(Vec::as_slice) && *insert_at == Some(index)
        }) {
            items.push(TreeItem::Ghost(depth));
        }

        let mut path = parent.map_or_else(Vec::new, Clone::clone);
        path.push(index);
        items.push(TreeItem::Node(path.clone()));
        walk_tree(items, ghost, Some(&path), &child.children, depth + 1);
    }

    if ghost.is_some_and(|(ghost_parent, insert_at)| {
        ghost_parent.as_deref() == parent.map(Vec::as_slice)
            && (insert_at.is_none() || *insert_at == Some(children.len()))
    }) {
        items.push(TreeItem::Ghost(depth));
    }
}

/// Rebuild the unfiltered tree and select `path` when it still exists.
pub fn rebuild_and_select(app: &mut AppState, todo_list: &TodoList, path: &NodePath) {
    app.tree_items = build_tree_items(todo_list, &app.mode);
    if let Some(position) = app
        .tree_items
        .iter()
        .position(|item| matches!(item, TreeItem::Node(p) if p == path))
    {
        app.left_state.select(Some(position));
    }
}

/// Path of the selected section, ignoring ghost rows.
pub fn selected_node(app: &AppState) -> Option<NodePath> {
    let index = app.left_state.selected()?;
    match app.tree_items.get(index)? {
        TreeItem::Node(path) => Some(path.clone()),
        TreeItem::Ghost(_) => None,
    }
}

/// Display name of a section path.
pub fn node_name(todo_list: &TodoList, node: &NodePath) -> String {
    get_node(todo_list, node).map_or_else(String::new, |section| section.name.clone())
}

/// Heading level of a section path.
pub fn node_level(todo_list: &TodoList, node: &[usize]) -> usize {
    get_node(todo_list, node).map_or(1, |section| section.level)
}

/// Make the left selection point at an existing row.
///
/// An empty tree clears the selection instead of leaving a dangling index.
pub fn clamp_left_selection(app: &mut AppState) {
    if app.tree_items.is_empty() {
        app.left_state.select(None);
    } else {
        let selected = app.left_state.selected().unwrap_or(0);
        app.left_state
            .select(Some(selected.min(app.tree_items.len() - 1)));
    }
}

/// Make the right selection point at an existing task row.
pub fn clamp_right_selection(app: &mut AppState, row_count: usize) {
    if row_count == 0 {
        app.right_state.select(None);
    } else {
        let selected = app.right_state.selected().unwrap_or(0);
        app.right_state.select(Some(selected.min(row_count - 1)));
    }
}
