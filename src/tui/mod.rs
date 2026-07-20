pub mod input;
pub mod render;
pub mod state;

use crate::parser::parse_file;
use crate::task::{NodePath, Section, Task, TodoList, get_node, get_node_mut};
use crate::tui::input::{InputDueParams, InputSectionParams, InputTaskParams};
use crate::tui::state::{AppState, Mode, TreeItem};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, stdout};
use std::path::Path;

pub fn run_tui(path: &Path) -> Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(out);
    let mut term = Terminal::new(backend).context("create terminal")?;

    // Guard struct ensures terminal cleanup even on panic
    struct TerminalGuard<'a> {
        term: &'a mut Terminal<CrosstermBackend<io::Stdout>>,
    }
    impl<'a> Drop for TerminalGuard<'a> {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(self.term.backend_mut(), LeaveAlternateScreen);
            let _ = self.term.show_cursor();
        }
    }

    let guard = TerminalGuard { term: &mut term };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_engine(guard.term, path)
    }));

    drop(guard); // Cleanup runs here

    match result {
        Ok(inner_result) => inner_result,
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            Err(anyhow::anyhow!("TUI panicked: {}", msg))
        }
    }
}

fn run_engine(term: &mut Terminal<CrosstermBackend<io::Stdout>>, path: &Path) -> Result<()> {
    let mut todo_list = parse_file(path)?;
    let mut app = AppState::new(todo_list.sections.len());

    loop {
        app.tree_items = build_visible_tree_items(&todo_list, &app.mode, &app.filter_lower);
        sync_selection_states(&mut app, &todo_list);
        term.draw(|f| render::draw_ui(f, &todo_list, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == event::KeyCode::Char('c')
            {
                break;
            }

            let quit = match &app.mode {
                Mode::Normal => input::handle_normal(&mut app, &mut todo_list, path, key.code)?,
                Mode::Help => {
                    if key.code == event::KeyCode::Esc
                        || key.code == event::KeyCode::Char('q')
                        || key.code == event::KeyCode::Char('?')
                    {
                        app.mode = Mode::Normal;
                    } else if key.code == event::KeyCode::Char('/') {
                        app.mode = Mode::Filter;
                        app.left_state.select(Some(0));
                        app.right_state.select(Some(0));
                    } else if key.code == event::KeyCode::Down || key.code == event::KeyCode::Char('j') {
                        app.help_scroll = app.help_scroll.saturating_add(1);
                    } else if key.code == event::KeyCode::Up || key.code == event::KeyCode::Char('k') {
                        app.help_scroll = app.help_scroll.saturating_sub(1);
                    }
                    false
                }
                Mode::InputTask {
                    editing_idx,
                    insert_idx,
                    above,
                    buf,
                    cursor,
                } => {
                    let mut params = InputTaskParams {
                        editing_idx: *editing_idx,
                        insert_idx: *insert_idx,
                        above: *above,
                        buf: buf.clone(),
                        cursor: *cursor,
                    };
                    input::handle_input_task(
                        &mut app,
                        &mut todo_list,
                        path,
                        key.code,
                        key.modifiers,
                        &mut params,
                    )?;
                    false
                }
                Mode::InputDue {
                    task_idx,
                    buf,
                    cursor,
                } => {
                    let mut params = InputDueParams {
                        task_idx: *task_idx,
                        buf: buf.clone(),
                        cursor: *cursor,
                    };
                    input::handle_input_due(&mut app, &mut todo_list, path, key.code, key.modifiers, &mut params)?;
                    false
                }
                Mode::InputSection {
                    node,
                    parent,
                    insert_idx,
                    buf,
                    cursor,
                } => {
                    let mut params = InputSectionParams {
                        node: node.clone(),
                        parent: parent.clone(),
                        insert_idx: *insert_idx,
                        buf: buf.clone(),
                        cursor: *cursor,
                    };
                    input::handle_input_section(
                        &mut app,
                        &mut todo_list,
                        path,
                        key.code,
                        key.modifiers,
                        &mut params,
                    )?;
                    false
                }
                Mode::Filter => {
                    input::handle_filter(&mut app, key.code)?;
                    false
                }
            };

            if quit {
                break;
            }
        }
    }
    Ok(())
}

fn sync_selection_states(app: &mut AppState, todo_list: &TodoList) {
    match &app.mode {
        Mode::InputSection { node: None, .. } => {
            if let Some(pos) = app
                .tree_items
                .iter()
                .position(|i| matches!(i, TreeItem::Ghost(_)))
            {
                app.left_state.select(Some(pos));
            }
        }
        Mode::InputTask {
            editing_idx: None,
            insert_idx,
            ..
        } => {
            if let Some(node) = selected_node(app) {
                let task_refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                let pos = insert_idx.unwrap_or(task_refs.len()).min(task_refs.len());
                app.right_state.select(Some(pos));
            }
        }
        _ => {}
    }
}

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

fn section_matches(sec: &Section, filter_lower: &str) -> bool {
    sec.name.to_lowercase().contains(filter_lower)
}

fn task_matches(task: &Task, filter_lower: &str) -> bool {
    if task.text.to_lowercase().contains(filter_lower) {
        return true;
    }
    task.due.is_some_and(|d| {
        d.format("%Y-%m-%d").to_string().to_lowercase().contains(filter_lower)
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

pub fn count_tasks(sec: &Section) -> usize {
    sec.count_tasks()
}

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

fn node_breadcrumb(todo_list: &TodoList, node: &NodePath) -> Option<String> {
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

pub fn get_task_from_ref<'a>(todo_list: &'a TodoList, ref_item: &TaskRef) -> Option<&'a Task> {
    let sec = get_node(todo_list, &ref_item.node)?;
    sec.tasks.get(ref_item.task_idx)
}

pub fn get_task_from_ref_mut<'a>(todo_list: &'a mut TodoList, ref_item: &TaskRef) -> Option<&'a mut Task> {
    let sec = get_node_mut(todo_list, &ref_item.node)?;
    sec.tasks.get_mut(ref_item.task_idx)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TodoList;
    use crate::tui::state::{AppState, Mode, TreeItem};

    fn sample_todo_list() -> TodoList {
        TodoList {
            sections: vec![
                Section {
                    name: "work".to_string(),
                    tasks: vec![
                        Task {
                            text: "Task A".to_string(),
                            is_done: false,
                            due: None,
                        },
                        Task {
                            text: "Task B".to_string(),
                            is_done: true,
                            due: None,
                        },
                    ],
                    children: vec![Section {
                        name: "backend".to_string(),
                        tasks: vec![Task {
                            text: "Fix API".to_string(),
                            is_done: false,
                            due: None,
                        }],
                        children: vec![Section {
                            name: "api".to_string(),
                            tasks: vec![Task {
                                text: "Fix endpoint".to_string(),
                                is_done: false,
                                due: None,
                            }],
                            children: vec![],
                        }],
                    }],
                },
                Section {
                    name: "personal".to_string(),
                    tasks: vec![Task {
                        text: "Buy milk".to_string(),
                        is_done: false,
                        due: None,
                    }],
                    children: vec![],
                },
            ],
        }
    }

    #[test]
    fn build_tree_items_in_normal_mode() {
        let list = sample_todo_list();
        let items = build_tree_items(&list, &Mode::Normal);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], TreeItem::Node(vec![0]));
        assert_eq!(items[1], TreeItem::Node(vec![0, 0]));
        assert_eq!(items[2], TreeItem::Node(vec![0, 0, 0]));
        assert_eq!(items[3], TreeItem::Node(vec![1]));
    }

    #[test]
    fn build_tree_items_empty_list() {
        let list = TodoList::default();
        let items = build_tree_items(&list, &Mode::Normal);
        assert!(items.is_empty());
    }

    #[test]
    fn selected_node_returns_none_when_nothing_selected() {
        let mut app = AppState::new(0);
        app.tree_items = vec![TreeItem::Node(vec![0])];
        assert!(selected_node(&app).is_none());
    }

    #[test]
    fn selected_node_returns_correct_node() {
        let mut app = AppState::new(2);
        app.tree_items = vec![
            TreeItem::Node(vec![0]),
            TreeItem::Node(vec![0, 0]),
            TreeItem::Node(vec![1]),
        ];
        app.left_state.select(Some(1));
        assert_eq!(selected_node(&app), Some(vec![0, 0]));
    }

    #[test]
    fn selected_node_returns_none_on_ghost() {
        let mut app = AppState::new(0);
        app.tree_items = vec![TreeItem::Ghost(0)];
        app.left_state.select(Some(0));
        assert!(selected_node(&app).is_none());
    }

    #[test]
    fn node_name_for_section() {
        let list = sample_todo_list();
        assert_eq!(node_name(&list, &vec![0]), "work");
        assert_eq!(node_name(&list, &vec![1]), "personal");
        assert_eq!(node_name(&list, &vec![0, 0]), "backend");
    }

    #[test]
    fn build_visible_tree_items_empty_filter_keeps_all() {
        let list = sample_todo_list();
        let items = build_visible_tree_items(&list, &Mode::Normal, "");
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn build_visible_tree_items_filters_sections_case_insensitive() {
        let list = sample_todo_list();
        // Filter is pre-lowercased by AppState, so pass lowercase here
        let items = build_visible_tree_items(&list, &Mode::Normal, "personal");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0], TreeItem::Node(vec![1]));
    }

    #[test]
    fn build_visible_tree_items_includes_matching_parent_of_nested_task() {
        let list = sample_todo_list();
        // "endpoint" only appears in the nested "api" task; its ancestors must stay visible.
        let items = build_visible_tree_items(&list, &Mode::Normal, "endpoint");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], TreeItem::Node(vec![0]));
        assert_eq!(items[1], TreeItem::Node(vec![0, 0]));
        assert_eq!(items[2], TreeItem::Node(vec![0, 0, 0]));
    }

    #[test]
    fn get_task_refs_filtered_narrows_by_text() {
        let list = sample_todo_list();
        let refs = get_task_refs_filtered(&list, &vec![0], "backend");
        // Matches the "backend" subsection task directly and the nested "api" task via its
        // breadcrumb (sub_name "backend › api").
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].node, vec![0, 0]);
        assert_eq!(refs[0].task_idx, 0);
        assert_eq!(refs[1].node, vec![0, 0, 0]);
        assert_eq!(refs[1].task_idx, 0);
    }

    #[test]
    fn get_task_refs_filtered_empty_filter_returns_all() {
        let list = sample_todo_list();
        let refs = get_task_refs_filtered(&list, &vec![0], "");
        assert_eq!(refs.len(), 4);
    }

    #[test]
    fn count_tasks_includes_all_descendants() {
        let list = sample_todo_list();
        assert_eq!(count_tasks(&list.sections[0]), 4);
    }

    #[test]
    fn count_tasks_section_only() {
        let list = sample_todo_list();
        assert_eq!(count_tasks(&list.sections[1]), 1);
    }

    #[test]
    fn count_tasks_empty_section() {
        let sec = Section {
            name: "empty".to_string(),
            tasks: Vec::new(),
            children: Vec::new(),
        };
        assert_eq!(count_tasks(&sec), 0);
    }

    #[test]
    fn task_refs_for_top_level_flattens_subtree() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, &vec![0]);
        assert_eq!(refs.len(), 4);
        assert_eq!(refs[0].node, vec![0]);
        assert_eq!(refs[0].task_idx, 0);
        assert_eq!(refs[0].sub_name, None);
        assert_eq!(refs[1].node, vec![0]);
        assert_eq!(refs[1].task_idx, 1);
        assert_eq!(refs[1].sub_name, None);
        assert_eq!(refs[2].node, vec![0, 0]);
        assert_eq!(refs[2].task_idx, 0);
        assert_eq!(refs[2].sub_name.as_deref(), Some("backend"));
        assert_eq!(refs[3].node, vec![0, 0, 0]);
        assert_eq!(refs[3].task_idx, 0);
        assert_eq!(refs[3].sub_name.as_deref(), Some("backend › api"));
    }

    #[test]
    fn task_refs_for_nested_node_lists_direct_only() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, &vec![0, 0]);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].node, vec![0, 0]);
        assert_eq!(refs[0].task_idx, 0);
        assert_eq!(refs[0].sub_name, None);
    }

    #[test]
    fn task_refs_for_out_of_bounds_section() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, &vec![99]);
        assert!(refs.is_empty());
    }

    #[test]
    fn get_task_from_ref_subsection_task() {
        let list = sample_todo_list();
        let r = TaskRef {
            node: vec![0, 0, 0],
            task_idx: 0,
            sub_name: Some("api".to_string()),
        };
        let task = get_task_from_ref(&list, &r).unwrap();
        assert_eq!(task.text, "Fix endpoint");
    }

    #[test]
    fn get_task_from_ref_mut_modifies_task() {
        let mut list = sample_todo_list();
        let r = TaskRef {
            node: vec![0],
            task_idx: 0,
            sub_name: None,
        };
        let task = get_task_from_ref_mut(&mut list, &r).unwrap();
        task.is_done = true;
        task.text = "Modified".to_string();
        assert!(list.sections[0].tasks[0].is_done);
        assert_eq!(list.sections[0].tasks[0].text, "Modified");
    }

    #[test]
    fn get_task_from_ref_mut_nested_subsection() {
        let mut list = sample_todo_list();
        let r = TaskRef {
            node: vec![0, 0, 0],
            task_idx: 0,
            sub_name: Some("api".to_string()),
        };
        let task = get_task_from_ref_mut(&mut list, &r).unwrap();
        task.text = "Updated endpoint".to_string();
        assert_eq!(
            list.sections[0].children[0].children[0].tasks[0].text,
            "Updated endpoint"
        );
    }

    #[test]
    fn insert_section_top_level() {
        let mut list = sample_todo_list();
        let path = insert_section(&mut list, None, 1, Section::new("inserted")).unwrap();
        assert_eq!(path, vec![1]);
        assert_eq!(list.sections.len(), 3);
        assert_eq!(list.sections[1].name, "inserted");
    }

    #[test]
    fn insert_section_nested() {
        let mut list = sample_todo_list();
        let path = insert_section(&mut list, Some(&vec![0]), 0, Section::new("child")).unwrap();
        assert_eq!(path, vec![0, 0]);
        assert_eq!(list.sections[0].children[0].name, "child");
        assert_eq!(list.sections[0].children[1].name, "backend");
    }

    #[test]
    fn remove_section_nested() {
        let mut list = sample_todo_list();
        let removed = remove_section(&mut list, &vec![0, 0]).unwrap();
        assert_eq!(removed.name, "backend");
        assert!(list.sections[0].children.is_empty());
    }

    #[test]
    fn child_count_helper() {
        let list = sample_todo_list();
        assert_eq!(child_count(&list, None), 2);
        assert_eq!(child_count(&list, Some(&vec![0])), 1);
        assert_eq!(child_count(&list, Some(&vec![0, 0])), 1);
    }
}
