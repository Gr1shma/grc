pub mod input;
pub mod render;
pub mod state;

use crate::parser::parse_file;
use crate::task::{Task, TodoList};
use crate::tui::state::{AppState, Mode, TreeNode};
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
        app.tree_nodes = build_tree_nodes(&todo_list, &app.mode);
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
                    }
                    false
                }
                Mode::InputTask {
                    editing_idx,
                    insert_idx,
                    mut buf,
                    mut cursor,
                } => {
                    input::handle_input_task(
                        &mut app,
                        &mut todo_list,
                        path,
                        key.code,
                        editing_idx,
                        insert_idx,
                        &mut buf,
                        &mut cursor,
                    )?;
                    false
                }
                Mode::InputDue {
                    task_idx,
                    mut buf,
                    mut cursor,
                } => {
                    input::handle_input_due(
                        &mut app,
                        &mut todo_list,
                        path,
                        key.code,
                        task_idx,
                        &mut buf,
                        &mut cursor,
                    )?;
                    false
                }
                Mode::InputSection {
                    node,
                    insert_idx,
                    mut buf,
                    mut cursor,
                } => {
                    input::handle_input_section(
                        &mut app,
                        &mut todo_list,
                        path,
                        key.code,
                        node,
                        insert_idx,
                        &mut buf,
                        &mut cursor,
                    )?;
                    false
                }
                Mode::InputSubsection {
                    parent_sec_idx,
                    insert_idx,
                    mut buf,
                    mut cursor,
                } => {
                    input::handle_input_subsection(
                        &mut app,
                        &mut todo_list,
                        path,
                        key.code,
                        parent_sec_idx,
                        insert_idx,
                        &mut buf,
                        &mut cursor,
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
            let ghost = TreeNode::Section(todo_list.sections.len());
            if let Some(pos) = app.tree_nodes.iter().position(|n| *n == ghost) {
                app.left_state.select(Some(pos));
            }
        }
        Mode::InputSubsection { parent_sec_idx, .. } => {
            if *parent_sec_idx < todo_list.sections.len() {
                let sub_len = todo_list.sections[*parent_sec_idx].subsections.len();
                let ghost = TreeNode::Subsection(*parent_sec_idx, sub_len);
                if let Some(pos) = app.tree_nodes.iter().position(|n| *n == ghost) {
                    app.left_state.select(Some(pos));
                }
            }
        }
        Mode::InputTask {
            editing_idx: None,
            insert_idx,
            ..
        } => {
            if let Some(node) = selected_node(app) {
                let task_refs = get_task_refs(todo_list, node);
                let pos = insert_idx.unwrap_or(task_refs.len()).min(task_refs.len());
                app.right_state.select(Some(pos));
            }
        }
        _ => {}
    }
}

pub fn build_tree_nodes(todo_list: &TodoList, mode: &Mode) -> Vec<TreeNode> {
    let mut nodes = Vec::new();

    for (s, sec) in todo_list.sections.iter().enumerate() {
        if let Mode::InputSection {
            node: None,
            insert_idx: Some(i),
            ..
        } = mode
            && *i == s
        {
            nodes.push(TreeNode::Section(todo_list.sections.len()));
        }
        nodes.push(TreeNode::Section(s));

        for (sb, _) in sec.subsections.iter().enumerate() {
            if let Mode::InputSubsection {
                parent_sec_idx,
                insert_idx: Some(i),
                ..
            } = mode
                && *parent_sec_idx == s
                && *i == sb
            {
                nodes.push(TreeNode::Subsection(s, sec.subsections.len()));
            }
            nodes.push(TreeNode::Subsection(s, sb));
        }
        if let Mode::InputSubsection {
            parent_sec_idx,
            insert_idx,
            ..
        } = mode
            && *parent_sec_idx == s
            && (insert_idx.is_none() || *insert_idx == Some(sec.subsections.len()))
        {
            nodes.push(TreeNode::Subsection(s, sec.subsections.len()));
        }
    }

    if let Mode::InputSection {
        node: None,
        insert_idx,
        ..
    } = mode
        && (insert_idx.is_none() || *insert_idx == Some(todo_list.sections.len()))
    {
        nodes.push(TreeNode::Section(todo_list.sections.len()));
    }

    nodes
}

pub fn selected_node(app: &AppState) -> Option<TreeNode> {
    let idx = app.left_state.selected()?;
    app.tree_nodes.get(idx).copied()
}

pub fn node_name(todo_list: &TodoList, node: TreeNode) -> String {
    match node {
        TreeNode::Section(s) => todo_list.sections[s].name.clone(),
        TreeNode::Subsection(s, sb) => todo_list.sections[s].subsections[sb].name.clone(),
    }
}

pub fn count_tasks_in_section(sec: &crate::task::Section) -> usize {
    let mut n = sec.tasks.len();
    for sub in &sec.subsections {
        n += sub.tasks.len();
    }
    n
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskRef {
    SectionTask {
        sec_idx: usize,
        task_idx: usize,
    },
    SubsectionTask {
        sec_idx: usize,
        sub_idx: usize,
        task_idx: usize,
    },
}

pub fn get_task_refs(todo_list: &TodoList, node: TreeNode) -> Vec<TaskRef> {
    let mut refs = Vec::new();
    match node {
        TreeNode::Section(s) => {
            if s < todo_list.sections.len() {
                let sec = &todo_list.sections[s];
                for t_idx in 0..sec.tasks.len() {
                    refs.push(TaskRef::SectionTask {
                        sec_idx: s,
                        task_idx: t_idx,
                    });
                }
                for (sb_idx, sub) in sec.subsections.iter().enumerate() {
                    for t_idx in 0..sub.tasks.len() {
                        refs.push(TaskRef::SubsectionTask {
                            sec_idx: s,
                            sub_idx: sb_idx,
                            task_idx: t_idx,
                        });
                    }
                }
            }
        }
        TreeNode::Subsection(s, sb) => {
            if s < todo_list.sections.len() && sb < todo_list.sections[s].subsections.len() {
                let sub = &todo_list.sections[s].subsections[sb];
                for t_idx in 0..sub.tasks.len() {
                    refs.push(TaskRef::SubsectionTask {
                        sec_idx: s,
                        sub_idx: sb,
                        task_idx: t_idx,
                    });
                }
            }
        }
    }
    refs
}

pub fn get_task_from_ref<'a>(todo_list: &'a TodoList, ref_item: &TaskRef) -> &'a Task {
    match ref_item {
        TaskRef::SectionTask { sec_idx, task_idx } => {
            &todo_list.sections[*sec_idx].tasks[*task_idx]
        }
        TaskRef::SubsectionTask {
            sec_idx,
            sub_idx,
            task_idx,
        } => &todo_list.sections[*sec_idx].subsections[*sub_idx].tasks[*task_idx],
    }
}

pub fn get_task_from_ref_mut<'a>(todo_list: &'a mut TodoList, ref_item: &TaskRef) -> &'a mut Task {
    match ref_item {
        TaskRef::SectionTask { sec_idx, task_idx } => {
            &mut todo_list.sections[*sec_idx].tasks[*task_idx]
        }
        TaskRef::SubsectionTask {
            sec_idx,
            sub_idx,
            task_idx,
        } => &mut todo_list.sections[*sec_idx].subsections[*sub_idx].tasks[*task_idx],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Section, Subsection, Task, TodoList};
    use crate::tui::state::{AppState, Mode, TreeNode};

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
                    subsections: vec![Subsection {
                        name: "backend".to_string(),
                        tasks: vec![Task {
                            text: "Fix API".to_string(),
                            is_done: false,
                            due: None,
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
                    subsections: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn build_tree_nodes_in_normal_mode() {
        let list = sample_todo_list();
        let nodes = build_tree_nodes(&list, &Mode::Normal);

        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0], TreeNode::Section(0));
        assert_eq!(nodes[1], TreeNode::Subsection(0, 0));
        assert_eq!(nodes[2], TreeNode::Section(1));
    }

    #[test]
    fn build_tree_nodes_empty_list() {
        let list = TodoList::default();
        let nodes = build_tree_nodes(&list, &Mode::Normal);
        assert!(nodes.is_empty());
    }

    #[test]
    fn selected_node_returns_none_when_nothing_selected() {
        let mut app = AppState::new(0);
        app.tree_nodes = vec![TreeNode::Section(0)];

        assert!(selected_node(&app).is_none());
    }

    #[test]
    fn selected_node_returns_correct_node() {
        let mut app = AppState::new(2);
        app.tree_nodes = vec![
            TreeNode::Section(0),
            TreeNode::Subsection(0, 0),
            TreeNode::Section(1),
        ];
        app.left_state.select(Some(1));
        assert_eq!(selected_node(&app), Some(TreeNode::Subsection(0, 0)));
    }

    #[test]
    fn node_name_for_section() {
        let list = sample_todo_list();
        assert_eq!(node_name(&list, TreeNode::Section(0)), "work");
        assert_eq!(node_name(&list, TreeNode::Section(1)), "personal");
    }

    #[test]
    fn node_name_for_subsection() {
        let list = sample_todo_list();
        assert_eq!(node_name(&list, TreeNode::Subsection(0, 0)), "backend");
    }

    #[test]
    fn count_tasks_includes_subsection_tasks() {
        let list = sample_todo_list();

        assert_eq!(count_tasks_in_section(&list.sections[0]), 3);
    }

    #[test]
    fn count_tasks_section_only() {
        let list = sample_todo_list();

        assert_eq!(count_tasks_in_section(&list.sections[1]), 1);
    }

    #[test]
    fn count_tasks_empty_section() {
        let sec = Section {
            name: "empty".to_string(),
            tasks: Vec::new(),
            subsections: Vec::new(),
        };
        assert_eq!(count_tasks_in_section(&sec), 0);
    }

    #[test]
    fn task_refs_for_section_includes_all_tasks() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, TreeNode::Section(0));

        assert_eq!(refs.len(), 3);
        assert_eq!(
            refs[0],
            TaskRef::SectionTask {
                sec_idx: 0,
                task_idx: 0
            }
        );
        assert_eq!(
            refs[1],
            TaskRef::SectionTask {
                sec_idx: 0,
                task_idx: 1
            }
        );
        assert_eq!(
            refs[2],
            TaskRef::SubsectionTask {
                sec_idx: 0,
                sub_idx: 0,
                task_idx: 0
            }
        );
    }

    #[test]
    fn task_refs_for_subsection() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, TreeNode::Subsection(0, 0));
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0],
            TaskRef::SubsectionTask {
                sec_idx: 0,
                sub_idx: 0,
                task_idx: 0
            }
        );
    }

    #[test]
    fn task_refs_for_out_of_bounds_section() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, TreeNode::Section(99));
        assert!(refs.is_empty());
    }

    #[test]
    fn task_refs_for_out_of_bounds_subsection() {
        let list = sample_todo_list();
        let refs = get_task_refs(&list, TreeNode::Subsection(0, 99));
        assert!(refs.is_empty());
    }

    #[test]
    fn get_task_from_ref_section_task() {
        let list = sample_todo_list();
        let r = TaskRef::SectionTask {
            sec_idx: 0,
            task_idx: 0,
        };
        let task = get_task_from_ref(&list, &r);
        assert_eq!(task.text, "Task A");
    }

    #[test]
    fn get_task_from_ref_subsection_task() {
        let list = sample_todo_list();
        let r = TaskRef::SubsectionTask {
            sec_idx: 0,
            sub_idx: 0,
            task_idx: 0,
        };
        let task = get_task_from_ref(&list, &r);
        assert_eq!(task.text, "Fix API");
    }

    #[test]
    fn get_task_from_ref_mut_modifies_task() {
        let mut list = sample_todo_list();
        let r = TaskRef::SectionTask {
            sec_idx: 0,
            task_idx: 0,
        };
        let task = get_task_from_ref_mut(&mut list, &r);
        task.is_done = true;
        task.text = "Modified".to_string();
        assert!(list.sections[0].tasks[0].is_done);
        assert_eq!(list.sections[0].tasks[0].text, "Modified");
    }

    #[test]
    fn get_task_from_ref_mut_subsection() {
        let mut list = sample_todo_list();
        let r = TaskRef::SubsectionTask {
            sec_idx: 0,
            sub_idx: 0,
            task_idx: 0,
        };
        let task = get_task_from_ref_mut(&mut list, &r);
        task.text = "Updated API".to_string();
        assert_eq!(list.sections[0].subsections[0].tasks[0].text, "Updated API");
    }
}
