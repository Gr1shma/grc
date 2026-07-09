use crate::parser::write_file;
use crate::task::TodoList;
use crate::tui::state::{AppState, ClipboardItem, Focus, Mode, TreeNode};
use crate::tui::{
    TaskRef, get_task_from_ref, get_task_from_ref_mut, get_task_refs, node_name, selected_node,
};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::path::Path;

pub fn handle_normal(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
) -> Result<bool> {
    if code == KeyCode::Char('?') {
        app.mode = Mode::Help;
        return Ok(false);
    }

    if app.pending_d {
        app.pending_d = false;
        if code == KeyCode::Char('d') {
            match app.focus {
                Focus::Left => delete_tree_node(app, todo_list, path)?,
                Focus::Right => delete_task(app, todo_list, path)?,
            }
            return Ok(false);
        }
    }

    if app.pending_g {
        app.pending_g = false;
        if code == KeyCode::Char('g') {
            match app.focus {
                Focus::Left => {
                    app.left_state.select(Some(0));
                    app.right_state.select(Some(0));
                }
                Focus::Right => {
                    app.right_state.select(Some(0));
                }
            }
            return Ok(false);
        }
    }

    if app.pending_y {
        app.pending_y = false;
        if code == KeyCode::Char('y') {
            match app.focus {
                Focus::Left => yank_tree_node(app, todo_list)?,
                Focus::Right => yank_task(app, todo_list)?,
            }
            return Ok(false);
        }
    }

    match app.focus {
        Focus::Left => match code {
            KeyCode::Char('q') => return Ok(true),

            KeyCode::Char('j') | KeyCode::Down => {
                let max = app.tree_nodes.len().saturating_sub(1);
                let cur = app.left_state.selected().unwrap_or(0);
                app.left_state.select(Some((cur + 1).min(max)));
                app.right_state.select(Some(0));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let cur = app.left_state.selected().unwrap_or(0);
                app.left_state.select(Some(cur.saturating_sub(1)));
                app.right_state.select(Some(0));
            }

            KeyCode::Char('l') | KeyCode::Tab | KeyCode::Enter => {
                if selected_node(app).is_some() {
                    app.focus = Focus::Right;
                }
            }

            KeyCode::Char('A') => {
                app.mode = Mode::InputSection {
                    node: None,
                    insert_idx: None,
                    buf: String::new(),
                };
            }

            KeyCode::Char('a') | KeyCode::Char('o') => {
                if let Some(node) = selected_node(app) {
                    let (parent_sec_idx, sub_idx) = match node {
                        TreeNode::Section(s) => (s, None),
                        TreeNode::Subsection(s, sb) => (s, Some(sb + 1)),
                    };
                    app.mode = Mode::InputSubsection {
                        parent_sec_idx,
                        insert_idx: sub_idx,
                        buf: String::new(),
                    };
                }
            }

            KeyCode::Char('O') => {
                if let Some(node) = selected_node(app) {
                    match node {
                        TreeNode::Section(s) => {
                            app.mode = Mode::InputSection {
                                node: None,
                                insert_idx: Some(s),
                                buf: String::new(),
                            };
                        }
                        TreeNode::Subsection(s, sb) => {
                            app.mode = Mode::InputSubsection {
                                parent_sec_idx: s,
                                insert_idx: Some(sb),
                                buf: String::new(),
                            };
                        }
                    }
                }
            }

            KeyCode::Char('i') => {
                if let Some(node) = selected_node(app) {
                    let name = node_name(todo_list, node);
                    app.mode = Mode::InputSection {
                        node: Some(node),
                        insert_idx: None,
                        buf: name,
                    };
                }
            }

            KeyCode::Char('d') => {
                app.pending_d = true;
            }

            KeyCode::Char('g') => {
                app.pending_g = true;
            }

            KeyCode::Char('y') => {
                app.pending_y = true;
            }

            KeyCode::Char('p') => {
                paste_clipboard(app, todo_list, path)?;
            }

            KeyCode::Char('G') => {
                let max = app.tree_nodes.len().saturating_sub(1);
                app.left_state.select(Some(max));
                app.right_state.select(Some(0));
            }

            _ => {}
        },

        Focus::Right => match code {
            KeyCode::Char('q') => return Ok(true),

            KeyCode::Char('h') | KeyCode::Esc | KeyCode::Tab => {
                app.focus = Focus::Left;
            }

            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(node) = selected_node(app) {
                    let max = get_task_refs(todo_list, node).len().saturating_sub(1);
                    let cur = app.right_state.selected().unwrap_or(0);
                    app.right_state.select(Some((cur + 1).min(max)));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let cur = app.right_state.selected().unwrap_or(0);
                app.right_state.select(Some(cur.saturating_sub(1)));
            }

            KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('x') => {
                if let Some(node) = selected_node(app) {
                    let cur = app.right_state.selected().unwrap_or(0);
                    let refs = get_task_refs(todo_list, node);
                    if let Some(ref_item) = refs.get(cur) {
                        let task = get_task_from_ref_mut(todo_list, ref_item);
                        task.is_done = !task.is_done;
                        write_file(path, todo_list)?;
                    }
                }
            }

            KeyCode::Char('a') | KeyCode::Char('o') => {
                let cur = app.right_state.selected().unwrap_or(0);
                app.mode = Mode::InputTask {
                    editing_idx: None,
                    insert_idx: Some(cur + 1),
                    buf: String::new(),
                };
            }
            KeyCode::Char('O') => {
                let cur = app.right_state.selected().unwrap_or(0);
                app.mode = Mode::InputTask {
                    editing_idx: None,
                    insert_idx: Some(cur),
                    buf: String::new(),
                };
            }
            KeyCode::Char('A') => {
                app.mode = Mode::InputTask {
                    editing_idx: None,
                    insert_idx: None,
                    buf: String::new(),
                };
            }

            KeyCode::Char('i') => {
                if let Some(node) = selected_node(app) {
                    let cur = app.right_state.selected().unwrap_or(0);
                    let refs = get_task_refs(todo_list, node);
                    if let Some(ref_item) = refs.get(cur) {
                        let task = get_task_from_ref(todo_list, ref_item);
                        app.mode = Mode::InputTask {
                            editing_idx: Some(cur),
                            insert_idx: None,
                            buf: task.text.clone(),
                        };
                    }
                }
            }

            KeyCode::Char('t') => {
                if let Some(node) = selected_node(app) {
                    let cur = app.right_state.selected().unwrap_or(0);
                    let refs = get_task_refs(todo_list, node);
                    if let Some(ref_item) = refs.get(cur) {
                        let task = get_task_from_ref(todo_list, ref_item);
                        let existing = task
                            .due
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default();
                        app.mode = Mode::InputDue {
                            task_idx: cur,
                            buf: existing,
                        };
                    }
                }
            }

            KeyCode::Char('d') => {
                app.pending_d = true;
            }

            KeyCode::Char('g') => {
                app.pending_g = true;
            }

            KeyCode::Char('y') => {
                app.pending_y = true;
            }

            KeyCode::Char('p') => {
                paste_clipboard(app, todo_list, path)?;
            }

            KeyCode::Char('G') => {
                if let Some(node) = selected_node(app) {
                    let max = get_task_refs(todo_list, node).len().saturating_sub(1);
                    app.right_state.select(Some(max));
                }
            }

            _ => {}
        },
    }
    Ok(false)
}

pub(crate) fn delete_task(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    if let Some(node) = selected_node(app) {
        let cur = app.right_state.selected().unwrap_or(0);
        let refs = get_task_refs(todo_list, node);
        if let Some(ref_item) = refs.get(cur) {
            let task = get_task_from_ref(todo_list, ref_item).clone();
            app.clipboard = Some(ClipboardItem::Task(task));

            match ref_item {
                TaskRef::SectionTask { sec_idx, task_idx } => {
                    todo_list.sections[*sec_idx].tasks.remove(*task_idx);
                }
                TaskRef::SubsectionTask {
                    sec_idx,
                    sub_idx,
                    task_idx,
                } => {
                    todo_list.sections[*sec_idx].subsections[*sub_idx]
                        .tasks
                        .remove(*task_idx);
                }
            }
            write_file(path, todo_list)?;
            let new_len = get_task_refs(todo_list, node).len();
            if new_len == 0 {
                app.right_state.select(Some(0));
                app.focus = Focus::Left;
            } else {
                app.right_state.select(Some(cur.min(new_len - 1)));
            }
        }
    }
    Ok(())
}

pub(crate) fn delete_tree_node(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    if let Some(node) = selected_node(app) {
        match node {
            TreeNode::Section(s) => {
                if s < todo_list.sections.len() {
                    let sec = todo_list.sections[s].clone();
                    app.clipboard = Some(ClipboardItem::Section(sec));

                    todo_list.sections.remove(s);
                    write_file(path, todo_list)?;
                    let new_len = todo_list.sections.len();
                    if new_len == 0 {
                        app.left_state.select(None);
                    } else {
                        app.left_state.select(Some(s.min(new_len - 1)));
                    }
                    app.right_state.select(Some(0));
                }
            }
            TreeNode::Subsection(s, sb) => {
                if s < todo_list.sections.len() {
                    let sec = &mut todo_list.sections[s];
                    if sb < sec.subsections.len() {
                        let sub = sec.subsections[sb].clone();
                        app.clipboard = Some(ClipboardItem::Subsection(sub));

                        sec.subsections.remove(sb);
                        write_file(path, todo_list)?;

                        let cur = app.left_state.selected().unwrap_or(0);
                        app.left_state.select(Some(cur.saturating_sub(1)));
                        app.right_state.select(Some(0));
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn yank_task(app: &mut AppState, todo_list: &TodoList) -> Result<()> {
    if let Some(node) = selected_node(app) {
        let cur = app.right_state.selected().unwrap_or(0);
        let refs = get_task_refs(todo_list, node);
        if let Some(ref_item) = refs.get(cur) {
            let task = get_task_from_ref(todo_list, ref_item).clone();
            app.clipboard = Some(ClipboardItem::Task(task));
        }
    }
    Ok(())
}

pub(crate) fn yank_tree_node(app: &mut AppState, todo_list: &TodoList) -> Result<()> {
    if let Some(node) = selected_node(app) {
        match node {
            TreeNode::Section(s) => {
                if s < todo_list.sections.len() {
                    let sec = todo_list.sections[s].clone();
                    app.clipboard = Some(ClipboardItem::Section(sec));
                }
            }
            TreeNode::Subsection(s, sb) => {
                if s < todo_list.sections.len() {
                    let sec = &todo_list.sections[s];
                    if sb < sec.subsections.len() {
                        let sub = sec.subsections[sb].clone();
                        app.clipboard = Some(ClipboardItem::Subsection(sub));
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn paste_clipboard(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    let clip = match &app.clipboard {
        Some(item) => item.clone(),
        None => return Ok(()),
    };

    match clip {
        ClipboardItem::Task(task) => {
            if let Some(node) = selected_node(app) {
                let refs = get_task_refs(todo_list, node);
                match node {
                    TreeNode::Section(s) => {
                        if s < todo_list.sections.len() {
                            if app.focus == Focus::Right {
                                let cur = app.right_state.selected().unwrap_or(0);
                                if refs.is_empty() {
                                    todo_list.sections[s].tasks.push(task);
                                    app.right_state.select(Some(0));
                                } else {
                                    let mut ins_idx = todo_list.sections[s].tasks.len();
                                    if cur < refs.len() {
                                        match refs[cur] {
                                            TaskRef::SectionTask { task_idx, .. } => {
                                                ins_idx = task_idx + 1;
                                            }
                                            TaskRef::SubsectionTask { .. } => {
                                                ins_idx = todo_list.sections[s].tasks.len();
                                            }
                                        }
                                    }
                                    todo_list.sections[s].tasks.insert(ins_idx, task);
                                    app.right_state.select(Some(ins_idx));
                                }
                            } else {
                                todo_list.sections[s].tasks.push(task);
                                app.right_state
                                    .select(Some(todo_list.sections[s].tasks.len() - 1));
                            }
                            write_file(path, todo_list)?;
                        }
                    }
                    TreeNode::Subsection(s, sb) => {
                        if s < todo_list.sections.len()
                            && sb < todo_list.sections[s].subsections.len()
                        {
                            if app.focus == Focus::Right {
                                let cur = app.right_state.selected().unwrap_or(0);
                                if refs.is_empty() {
                                    todo_list.sections[s].subsections[sb].tasks.push(task);
                                    app.right_state.select(Some(0));
                                } else {
                                    let mut ins_idx =
                                        todo_list.sections[s].subsections[sb].tasks.len();
                                    if cur < refs.len()
                                        && let TaskRef::SubsectionTask { task_idx, .. } = refs[cur]
                                    {
                                        ins_idx = task_idx + 1;
                                    }
                                    todo_list.sections[s].subsections[sb]
                                        .tasks
                                        .insert(ins_idx, task);
                                    app.right_state.select(Some(ins_idx));
                                }
                            } else {
                                todo_list.sections[s].subsections[sb].tasks.push(task);
                                app.right_state.select(Some(
                                    todo_list.sections[s].subsections[sb].tasks.len() - 1,
                                ));
                            }
                            write_file(path, todo_list)?;
                        }
                    }
                }
            }
        }
        ClipboardItem::Section(sec) => {
            if let Some(node) = selected_node(app) {
                let target_s = match node {
                    TreeNode::Section(s) => s,
                    TreeNode::Subsection(s, _) => s,
                };
                let ins_idx = (target_s + 1).min(todo_list.sections.len());
                todo_list.sections.insert(ins_idx, sec);
                write_file(path, todo_list)?;
                app.mode = Mode::Normal;
                let temp = crate::tui::build_tree_nodes(todo_list, &app.mode);
                if let Some(pos) = temp
                    .iter()
                    .position(|n| matches!(n, TreeNode::Section(s) if *s == ins_idx))
                {
                    app.left_state.select(Some(pos));
                }
            } else {
                todo_list.sections.push(sec);
                write_file(path, todo_list)?;
                app.left_state.select(Some(todo_list.sections.len() - 1));
            }
        }
        ClipboardItem::Subsection(sub) => {
            if let Some(node) = selected_node(app) {
                match node {
                    TreeNode::Section(s) => {
                        if s < todo_list.sections.len() {
                            todo_list.sections[s].subsections.push(sub);
                            write_file(path, todo_list)?;
                            app.mode = Mode::Normal;
                            let temp = crate::tui::build_tree_nodes(todo_list, &app.mode);
                            let target_sub_idx = todo_list.sections[s].subsections.len() - 1;
                            if let Some(pos) = temp.iter().position(|n| matches!(n, TreeNode::Subsection(sec_idx, sub_idx) if *sec_idx == s && *sub_idx == target_sub_idx)) {
                                app.left_state.select(Some(pos));
                            }
                        }
                    }
                    TreeNode::Subsection(s, sb) => {
                        if s < todo_list.sections.len() {
                            let sec = &mut todo_list.sections[s];
                            let ins_idx = (sb + 1).min(sec.subsections.len());
                            sec.subsections.insert(ins_idx, sub);
                            write_file(path, todo_list)?;
                            app.mode = Mode::Normal;
                            let temp = crate::tui::build_tree_nodes(todo_list, &app.mode);
                            if let Some(pos) = temp.iter().position(|n| matches!(n, TreeNode::Subsection(sec_idx, sub_idx) if *sec_idx == s && *sub_idx == ins_idx)) {
                                app.left_state.select(Some(pos));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;
    use crate::task::TodoList;
    use crate::tui::build_tree_nodes;
    use crate::tui::state::{AppState, ClipboardItem, Focus};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn setup(content: &str) -> (NamedTempFile, AppState, TodoList) {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        let list = parse_file(f.path()).unwrap();
        let mut app = AppState::new(list.sections.len());
        app.tree_nodes = build_tree_nodes(&list, &app.mode);
        (f, app, list)
    }

    #[test]
    fn test_yank_and_paste_task() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task A\n- [ ] Task B\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));

        yank_task(&mut app, &list).unwrap();
        assert!(matches!(app.clipboard, Some(ClipboardItem::Task(_))));
        if let Some(ClipboardItem::Task(ref t)) = app.clipboard {
            assert_eq!(t.text, "Task A");
        }

        paste_clipboard(&mut app, &mut list, f.path()).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks.len(), 3);
        assert_eq!(parsed.sections[0].tasks[0].text, "Task A");
        assert_eq!(parsed.sections[0].tasks[1].text, "Task A");
        assert_eq!(parsed.sections[0].tasks[2].text, "Task B");
    }

    #[test]
    fn test_delete_yanks_task() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task A\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));

        delete_task(&mut app, &mut list, f.path()).unwrap();

        let parsed = parse_file(f.path()).unwrap();
        assert!(parsed.sections[0].tasks.is_empty());

        assert!(matches!(app.clipboard, Some(ClipboardItem::Task(_))));
        if let Some(ClipboardItem::Task(ref t)) = app.clipboard {
            assert_eq!(t.text, "Task A");
        }
    }

    #[test]
    fn test_yank_and_paste_section() {
        let (f, mut app, mut list) = setup("# main\n# clz\n");
        app.focus = Focus::Left;
        app.left_state.select(Some(0));

        yank_tree_node(&mut app, &list).unwrap();
        assert!(matches!(app.clipboard, Some(ClipboardItem::Section(_))));

        app.left_state.select(Some(1));
        app.tree_nodes = build_tree_nodes(&list, &app.mode);
        paste_clipboard(&mut app, &mut list, f.path()).unwrap();

        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 3);
        assert_eq!(parsed.sections[0].name, "main");
        assert_eq!(parsed.sections[1].name, "clz");
        assert_eq!(parsed.sections[2].name, "main");
    }
}
