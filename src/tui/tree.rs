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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Task;

    fn sample() -> TodoList {
        TodoList {
            sections: vec![
                Section {
                    name: "work".into(),
                    level: 1,
                    tasks: vec![
                        Task {
                            text: "Task A".into(),
                            is_done: false,
                            due: None,
                            leading_raw: Some(Vec::new()),
                        },
                        Task {
                            text: "Task B".into(),
                            is_done: true,
                            due: None,
                            leading_raw: Some(Vec::new()),
                        },
                    ],
                    children: vec![Section {
                        name: "backend".into(),
                        level: 2,
                        tasks: vec![Task {
                            text: "Fix API".into(),
                            is_done: false,
                            due: None,
                            leading_raw: Some(Vec::new()),
                        }],
                        children: vec![Section {
                            name: "api".into(),
                            level: 3,
                            tasks: vec![Task {
                                text: "Fix endpoint".into(),
                                is_done: false,
                                due: None,
                                leading_raw: Some(Vec::new()),
                            }],
                            children: Vec::new(),
                            leading_raw: Some(Vec::new()),
                        }],
                        leading_raw: Some(Vec::new()),
                    }],
                    leading_raw: Some(Vec::new()),
                },
                Section {
                    name: "personal".into(),
                    level: 1,
                    tasks: vec![Task {
                        text: "Buy milk".into(),
                        is_done: false,
                        due: None,
                        leading_raw: Some(Vec::new()),
                    }],
                    children: Vec::new(),
                    leading_raw: Some(Vec::new()),
                },
            ],
            ..TodoList::default()
        }
    }

    #[test]
    fn normal_tree_is_depth_first() {
        let items = build_tree_items(&sample(), &Mode::Normal);
        assert_eq!(
            items,
            vec![
                TreeItem::Node(vec![0]),
                TreeItem::Node(vec![0, 0]),
                TreeItem::Node(vec![0, 0, 0]),
                TreeItem::Node(vec![1]),
            ]
        );
    }

    #[test]
    fn empty_tree() {
        assert!(build_tree_items(&TodoList::default(), &Mode::Normal).is_empty());
    }

    #[test]
    fn filter_by_section_name_case_insensitively() {
        let items = build_visible_tree_items(&sample(), &Mode::Normal, "personal");
        assert_eq!(items, vec![TreeItem::Node(vec![1])]);
    }

    #[test]
    fn filter_keeps_ancestors_of_matching_task() {
        let items = build_visible_tree_items(&sample(), &Mode::Normal, "endpoint");
        assert_eq!(
            items,
            vec![
                TreeItem::Node(vec![0]),
                TreeItem::Node(vec![0, 0]),
                TreeItem::Node(vec![0, 0, 0]),
            ]
        );
    }

    #[test]
    fn section_name_matches_take_priority_over_task_matches() {
        let mut list = sample();
        list.sections[0].tasks[0].text = "personal notes".into();
        let items = build_visible_tree_items(&list, &Mode::Normal, "personal");
        assert_eq!(items, vec![TreeItem::Node(vec![1])]);
    }

    #[test]
    fn filter_by_due_date() {
        let mut list = sample();
        list.sections[1].tasks[0].due = Some(chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap());
        let items = build_visible_tree_items(&list, &Mode::Normal, "2025-06");
        assert_eq!(items, vec![TreeItem::Node(vec![1])]);
    }

    #[test]
    fn selected_node_ignores_missing_and_ghost_rows() {
        let mut app = AppState::new(0);
        app.tree_items = vec![TreeItem::Node(vec![0]), TreeItem::Ghost(0)];
        assert!(selected_node(&app).is_none());

        app.left_state.select(Some(0));
        assert_eq!(selected_node(&app), Some(vec![0]));

        app.left_state.select(Some(1));
        assert!(selected_node(&app).is_none());

        app.left_state.select(Some(99));
        assert!(selected_node(&app).is_none());
    }

    #[test]
    fn clamp_clears_dangling_selection() {
        let mut app = AppState::new(0);
        app.tree_items.clear();
        app.left_state.select(Some(4));
        clamp_left_selection(&mut app);
        assert_eq!(app.left_state.selected(), None);

        app.tree_items = vec![TreeItem::Node(vec![0])];
        app.left_state.select(Some(4));
        clamp_left_selection(&mut app);
        assert_eq!(app.left_state.selected(), Some(0));

        app.right_state.select(Some(7));
        clamp_right_selection(&mut app, 2);
        assert_eq!(app.right_state.selected(), Some(1));
        clamp_right_selection(&mut app, 0);
        assert_eq!(app.right_state.selected(), None);
    }

    #[test]
    fn ghost_is_inserted_at_requested_position() {
        let list = sample();
        let input = crate::tui::state::SectionInput {
            node: None,
            parent: Some(vec![0]),
            insert_idx: Some(0),
            level: 2,
            field: crate::tui::state::TextField::default(),
        };
        let items = build_tree_items(&list, &Mode::InputSection(input));
        assert_eq!(items[1], TreeItem::Ghost(1));
    }

    #[test]
    fn heading_level_lookup() {
        let list = sample();
        assert_eq!(node_level(&list, &[0]), 1);
        assert_eq!(node_level(&list, &[0, 0]), 2);
        assert_eq!(node_level(&list, &[0, 0, 0]), 3);
        assert_eq!(node_level(&list, &[99]), 1);
    }
}
