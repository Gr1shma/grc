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

    let result = run_engine(&mut term, path);

    disable_raw_mode().context("disable raw mode")?;
    execute!(term.backend_mut(), LeaveAlternateScreen).context("leave alternate screen")?;
    term.show_cursor().context("restore cursor")?;
    result
}

fn run_engine(term: &mut Terminal<CrosstermBackend<io::Stdout>>, path: &Path) -> Result<()> {
    let mut todo_list = parse_file(path)?;
    let mut app = AppState::new(todo_list.sections.len());

    loop {
        app.tree_items = build_tree_items(&todo_list, &app.mode);
        sync_selection_states(&mut app, &todo_list);
        term.draw(|f| render::draw_ui(f, &todo_list, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == event::KeyCode::Char('c')
            {
                break;
            }

            let quit = match app.mode.clone() {
                Mode::Normal => input::handle_normal(&mut app, &mut todo_list, path, key.code)?,
                Mode::Help => {
                    if key.code == event::KeyCode::Esc
                        || key.code == event::KeyCode::Char('q')
                        || key.code == event::KeyCode::Char('?')
                    {
                        app.mode = Mode::Normal;
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
                        editing_idx,
                        insert_idx,
                        above,
                        buf,
                        cursor,
                    };
                    input::handle_input_task(
                        &mut app,
                        &mut todo_list,
                        path,
                        key.code,
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
                        task_idx,
                        buf,
                        cursor,
                    };
                    input::handle_input_due(&mut app, &mut todo_list, path, key.code, &mut params)?;
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
                        node,
                        parent,
                        insert_idx,
                        buf,
                        cursor,
                    };
                    input::handle_input_section(
                        &mut app,
                        &mut todo_list,
                        path,
                        key.code,
                        &mut params,
                    )?;
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
                let task_refs = get_task_refs(todo_list, &node);
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

pub fn get_task_from_ref<'a>(todo_list: &'a TodoList, ref_item: &TaskRef) -> &'a Task {
    &get_node(todo_list, &ref_item.node)
        .expect("task ref node must exist")
        .tasks[ref_item.task_idx]
}

pub fn get_task_from_ref_mut<'a>(todo_list: &'a mut TodoList, ref_item: &TaskRef) -> &'a mut Task {
    &mut get_node_mut(todo_list, &ref_item.node)
        .expect("task ref node must exist")
        .tasks[ref_item.task_idx]
}

pub fn insert_section(
    todo_list: &mut TodoList,
    parent: Option<&NodePath>,
    idx: usize,
    section: Section,
) -> NodePath {
    match parent {
        None => {
            let i = idx.min(todo_list.sections.len());
            todo_list.sections.insert(i, section);
            vec![i]
        }
        Some(p) => {
            let sec = get_node_mut(todo_list, p).expect("parent must exist");
            let i = idx.min(sec.children.len());
            sec.children.insert(i, section);
            let mut path = p.clone();
            path.push(i);
            path
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
        let task = get_task_from_ref(&list, &r);
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
        let task = get_task_from_ref_mut(&mut list, &r);
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
        let task = get_task_from_ref_mut(&mut list, &r);
        task.text = "Updated endpoint".to_string();
        assert_eq!(
            list.sections[0].children[0].children[0].tasks[0].text,
            "Updated endpoint"
        );
    }

    #[test]
    fn insert_section_top_level() {
        let mut list = sample_todo_list();
        let path = insert_section(&mut list, None, 1, Section::new("inserted"));
        assert_eq!(path, vec![1]);
        assert_eq!(list.sections.len(), 3);
        assert_eq!(list.sections[1].name, "inserted");
    }

    #[test]
    fn insert_section_nested() {
        let mut list = sample_todo_list();
        let path = insert_section(&mut list, Some(&vec![0]), 0, Section::new("child"));
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
