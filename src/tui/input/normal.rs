use crate::parser::write_file;
use crate::task::{NodePath, TodoList};
use crate::tui::state::{AppState, ClipboardItem, Focus, Mode};
use crate::tui::{
    build_tree_items, child_count, get_task_from_ref, get_task_from_ref_mut,
    get_task_refs_filtered, insert_section, node_name, rebuild_and_select, remove_section,
    selected_node,
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

    if code == KeyCode::Char(':') && app.focus == Focus::Right {
        sort_tasks_by_due(app, todo_list, path)?;
        return Ok(false);
    }

    if code == KeyCode::Char('/') {
        app.mode = Mode::Filter;
        return Ok(false);
    }

    if code == KeyCode::Esc && !app.filter.is_empty() {
        app.filter.clear();
        app.filter_lower.clear();
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
        // Mismatched key: cancel pending operation, discard the key
        return Ok(false);
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
        // Mismatched key: cancel pending operation, discard the key
        return Ok(false);
    }

    if app.pending_y {
        app.pending_y = false;
        if code == KeyCode::Char('y') {
            match app.focus {
                Focus::Left => yank_tree_node(app, todo_list),
                Focus::Right => yank_task(app, todo_list),
            }
            return Ok(false);
        }
        // Mismatched key: cancel pending operation, discard the key
        return Ok(false);
    }

    match app.focus {
        Focus::Left => handle_normal_left(app, todo_list, path, code),
        Focus::Right => handle_normal_right(app, todo_list, path, code),
    }
}

pub fn handle_filter(app: &mut AppState, code: KeyCode) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.filter.clear();
            app.filter_lower.clear();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            app.filter.pop();
            app.filter_lower = app.filter.to_lowercase();
        }
        KeyCode::Char(c) => {
            app.filter.push(c);
            app.filter_lower = app.filter.to_lowercase();
        }
        _ => {}
    }
    Ok(())
}

fn handle_normal_left(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
) -> Result<bool> {
    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('j') | KeyCode::Down => {
            let max = app.tree_items.len().saturating_sub(1);
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
        KeyCode::Char('a' | 'A' | 'o' | 'O') => {
            handle_normal_left_add_section(app, code);
        }
        KeyCode::Char('i') => {
            if let Some(node) = selected_node(app) {
                let name = node_name(todo_list, &node);
                let cursor = name.chars().count();
                app.mode = Mode::InputSection {
                    node: Some(node),
                    parent: None,
                    insert_idx: None,
                    buf: name,
                    cursor,
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
            paste_clipboard(app, todo_list, path, false)?;
        }
        KeyCode::Char('P') => {
            paste_clipboard(app, todo_list, path, true)?;
        }
        KeyCode::Char('G') => {
            let max = app.tree_items.len().saturating_sub(1);
            app.left_state.select(Some(max));
            app.right_state.select(Some(0));
        }
        _ => {}
    }
    Ok(false)
}

fn handle_normal_left_add_section(app: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Char('a') => {
            let parent_opt = selected_node(app);
            app.mode = Mode::InputSection {
                node: None,
                parent: parent_opt,
                insert_idx: Some(0),
                buf: String::new(),
                cursor: 0,
            };
        }
        KeyCode::Char('A') => {
            let path_sel = selected_node(app).unwrap_or_default();
            let len = path_sel.len();
            let (grandparent, insert_idx) = if len == 0 {
                (None, None)
            } else if len == 1 {
                (None, Some(path_sel[0]))
            } else {
                let gp = path_sel[..len - 2].to_vec();
                let gp_opt = if gp.is_empty() { None } else { Some(gp) };
                (gp_opt, Some(path_sel[len - 2]))
            };
            app.mode = Mode::InputSection {
                node: None,
                parent: grandparent,
                insert_idx,
                buf: String::new(),
                cursor: 0,
            };
        }
        KeyCode::Char('o') => {
            let path_sel = selected_node(app).unwrap_or_default();
            let len = path_sel.len();
            let parent = (len > 1).then(|| path_sel[..len - 1].to_vec());
            let insert_idx = (len > 0).then(|| path_sel[len - 1] + 1);
            app.mode = Mode::InputSection {
                node: None,
                parent,
                insert_idx,
                buf: String::new(),
                cursor: 0,
            };
        }
        KeyCode::Char('O') => {
            let path_sel = selected_node(app).unwrap_or_default();
            let len = path_sel.len();
            let parent = (len > 1).then(|| path_sel[..len - 1].to_vec());
            let insert_idx = (len > 0).then_some(path_sel[len - 1]);
            app.mode = Mode::InputSection {
                node: None,
                parent,
                insert_idx,
                buf: String::new(),
                cursor: 0,
            };
        }
        _ => {}
    }
}

fn handle_normal_right(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
) -> Result<bool> {
    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Tab => {
            app.focus = Focus::Left;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(node) = selected_node(app) {
                let max = get_task_refs_filtered(todo_list, &node, &app.filter_lower).len().saturating_sub(1);
                let cur = app.right_state.selected().unwrap_or(0);
                app.right_state.select(Some((cur + 1).min(max)));
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let cur = app.right_state.selected().unwrap_or(0);
            app.right_state.select(Some(cur.saturating_sub(1)));
        }
        KeyCode::Char(' ' | 'x') | KeyCode::Enter => {
            if let Some(node) = selected_node(app) {
                let cur = app.right_state.selected().unwrap_or(0);
                let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                if let Some(ref_item) = refs.get(cur) {
                    if let Some(task) = get_task_from_ref_mut(todo_list, ref_item) {
                        task.is_done = !task.is_done;
                    }
                    write_file(path, todo_list)?;
                }
            }
        }
        KeyCode::Char('a' | 'o' | 'O' | 'A') => {
            handle_normal_right_add_task(app, code);
        }
        KeyCode::Char('i') => {
            if let Some(node) = selected_node(app) {
                let cur = app.right_state.selected().unwrap_or(0);
                let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                if let Some(ref_item) = refs.get(cur) {
                    if let Some(task) = get_task_from_ref(todo_list, ref_item) {
                        let cursor = task.text.chars().count();
                        app.mode = Mode::InputTask {
                            editing_idx: Some(cur),
                            insert_idx: None,
                            above: false,
                            buf: task.text.clone(),
                            cursor,
                        };
                    }
                }
            }
        }
        KeyCode::Char('t') => {
            if let Some(node) = selected_node(app) {
                let cur = app.right_state.selected().unwrap_or(0);
                let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                if let Some(ref_item) = refs.get(cur) {
                    if let Some(task) = get_task_from_ref(todo_list, ref_item) {
                        let existing = task
                            .due
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default();
                        let cursor = existing.chars().count();
                        app.mode = Mode::InputDue {
                            task_idx: cur,
                            buf: existing,
                            cursor,
                        };
                    }
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
            paste_clipboard(app, todo_list, path, false)?;
        }
        KeyCode::Char('P') => {
            paste_clipboard(app, todo_list, path, true)?;
        }
        KeyCode::Char('G') => {
            if let Some(node) = selected_node(app) {
                let max = get_task_refs_filtered(todo_list, &node, &app.filter_lower).len().saturating_sub(1);
                app.right_state.select(Some(max));
            }
        }
        _ => {}
    }
    Ok(false)
}

fn handle_normal_right_add_task(app: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Char('a' | 'o') => {
            let cur = app.right_state.selected().unwrap_or(0);
            app.mode = Mode::InputTask {
                editing_idx: None,
                insert_idx: Some(cur + 1),
                above: false,
                buf: String::new(),
                cursor: 0,
            };
        }
        KeyCode::Char('O') => {
            let cur = app.right_state.selected().unwrap_or(0);
            app.mode = Mode::InputTask {
                editing_idx: None,
                insert_idx: Some(cur),
                above: true,
                buf: String::new(),
                cursor: 0,
            };
        }
        KeyCode::Char('A') => {
            app.mode = Mode::InputTask {
                editing_idx: None,
                insert_idx: None,
                above: false,
                buf: String::new(),
                cursor: 0,
            };
        }
        _ => {}
    }
}

pub fn delete_task(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    if let Some(node) = selected_node(app) {
        let cur = app.right_state.selected().unwrap_or(0);
        let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
        if let Some(ref_item) = refs.get(cur) {
            let task = match get_task_from_ref(todo_list, ref_item) {
                Some(t) => t.clone(),
                None => return Ok(()),
            };
            app.clipboard = Some(ClipboardItem::Task(task));

            let node_clone = ref_item.node.clone();
            let task_idx = ref_item.task_idx;
            if let Some(sec) = crate::task::get_node_mut(todo_list, &node_clone) {
                sec.tasks.remove(task_idx);
            }
            write_file(path, todo_list)?;
            let new_len = get_task_refs_filtered(todo_list, &node, &app.filter_lower).len();
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

pub fn delete_tree_node(app: &mut AppState, todo_list: &mut TodoList, path: &Path) -> Result<()> {
    if let Some(node) = selected_node(app)
        && let Some(removed) = remove_section(todo_list, &node)
    {
        app.clipboard = Some(ClipboardItem::Section(removed));
        write_file(path, todo_list)?;
        let cur = app.left_state.selected().unwrap_or(0);
        if app.tree_items.len() <= 1 {
            app.left_state.select(None);
        } else {
            app.left_state
                .select(Some(cur.saturating_sub(1).min(app.tree_items.len() - 1)));
        }
        app.right_state.select(Some(0));
    }
    Ok(())
}

pub fn yank_task(app: &mut AppState, todo_list: &TodoList) {
    if let Some(node) = selected_node(app) {
        let cur = app.right_state.selected().unwrap_or(0);
        let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
        if let Some(ref_item) = refs.get(cur) {
            let task = match get_task_from_ref(todo_list, ref_item) {
                Some(t) => t.clone(),
                None => return,
            };
            app.clipboard = Some(ClipboardItem::Task(task));
        }
    }
}

pub fn yank_tree_node(app: &mut AppState, todo_list: &TodoList) {
    if let Some(node) = selected_node(app)
        && let Some(sec) = crate::task::get_node(todo_list, &node)
    {
        app.clipboard = Some(ClipboardItem::Section(sec.clone()));
    }
}

pub fn paste_clipboard(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    above: bool,
) -> Result<()> {
    let clip = match &app.clipboard {
        Some(item) => item.clone(),
        None => return Ok(()),
    };

    match clip {
        ClipboardItem::Task(task) => {
            if let Some(node) = selected_node(app) {
                let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
                let cur = app.right_state.selected().unwrap_or(0);

                let (target_node, ins_idx) = if app.focus == Focus::Right {
                    if refs.is_empty() {
                        let n = child_or_self_task_count(todo_list, &node);
                        (node, if above { 0 } else { n })
                    } else if cur < refs.len() {
                        let r = &refs[cur];
                        let base = r.task_idx;
                        (r.node.clone(), if above { base } else { base + 1 })
                    } else {
                        let n =
                            crate::task::get_node(todo_list, &node).map_or(0, |s| s.tasks.len());
                        (node, n)
                    }
                } else {
                    let n = crate::task::get_node(todo_list, &node).map_or(0, |s| s.tasks.len());
                    (node, if above { 0 } else { n })
                };

                if let Some(sec) = crate::task::get_node_mut(todo_list, &target_node) {
                    sec.tasks.insert(ins_idx, task);
                }
                write_file(path, todo_list)?;

                if app.focus == Focus::Right {
                    app.tree_items = build_tree_items(todo_list, &app.mode);
                    if let Some(pos) = refs
                        .iter()
                        .position(|r| r.node == target_node && r.task_idx == ins_idx)
                    {
                        app.right_state.select(Some(pos));
                    } else {
                        app.right_state.select(Some(cur));
                    }
                }
            }
        }
        ClipboardItem::Section(sec) => {
            if let Some(node) = selected_node(app) {
                let len = node.len();
                let (parent_opt, idx) = if len == 0 {
                    (None, if above { 0 } else { todo_list.sections.len() })
                } else {
                    let parent = (len > 1).then(|| node[..len - 1].to_vec());
                    let base = node[len - 1];
                    let idx = if above {
                        base
                    } else {
                        (base + 1).min(child_count(todo_list, parent.as_ref()))
                    };
                    (parent, idx)
                };
                if let Ok(new_path) = insert_section(todo_list, parent_opt.as_ref(), idx, sec) {
                    let _ = write_file(path, todo_list);
                    app.mode = Mode::Normal;
                    rebuild_and_select(app, todo_list, &new_path);
                }
                app.right_state.select(Some(0));
            } else {
                let idx = if above { 0 } else { todo_list.sections.len() };
                if let Ok(new_path) = insert_section(todo_list, None, idx, sec) {
                    let _ = write_file(path, todo_list);
                    rebuild_and_select(app, todo_list, &new_path);
                }
            }
        }
    }
    Ok(())
}

fn child_or_self_task_count(todo_list: &TodoList, node: &NodePath) -> usize {
    crate::task::get_node(todo_list, node).map_or(0, |s| s.tasks.len())
}

fn sort_tasks_by_due(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
) -> Result<()> {
    if let Some(node) = selected_node(app) {
        if let Some(sec) = crate::task::get_node_mut(todo_list, &node) {
            sec.tasks.sort_by(|a, b| {
                match (a.due, b.due) {
                    (Some(da), Some(db)) => da.cmp(&db),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });
            write_file(path, todo_list)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;
    use crate::tui::state::{AppState, Focus, TreeItem};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn setup(content: &str) -> (NamedTempFile, AppState, TodoList) {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        let list = parse_file(f.path()).unwrap();
        let mut app = AppState::new(list.sections.len());
        app.tree_items = build_tree_items(&list, &app.mode);
        (f, app, list)
    }

    #[test]
    fn test_yank_and_paste_task() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task A\n- [ ] Task B\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));

        yank_task(&mut app, &list);
        assert!(matches!(app.clipboard, Some(ClipboardItem::Task(_))));
        if let Some(ClipboardItem::Task(ref t)) = app.clipboard {
            assert_eq!(t.text, "Task A");
        }

        paste_clipboard(&mut app, &mut list, f.path(), false).unwrap();
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

        yank_tree_node(&mut app, &list);
        assert!(matches!(app.clipboard, Some(ClipboardItem::Section(_))));

        app.left_state.select(Some(1));
        app.tree_items = build_tree_items(&list, &app.mode);
        paste_clipboard(&mut app, &mut list, f.path(), false).unwrap();

        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 3);
        assert_eq!(parsed.sections[0].name, "main");
        assert_eq!(parsed.sections[1].name, "clz");
        assert_eq!(parsed.sections[2].name, "main");
    }

    #[test]
    fn test_paste_task_above() {
        let (f, mut app, mut list) = setup("# main\n- [ ] A\n- [ ] B\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(1));

        yank_task(&mut app, &list);
        assert!(matches!(app.clipboard, Some(ClipboardItem::Task(_))));

        paste_clipboard(&mut app, &mut list, f.path(), true).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks.len(), 3);
        assert_eq!(parsed.sections[0].tasks[0].text, "A");
        assert_eq!(parsed.sections[0].tasks[1].text, "B");
        assert_eq!(parsed.sections[0].tasks[2].text, "B");
    }

    #[test]
    fn test_paste_section_above() {
        let (f, mut app, mut list) = setup("# main\n# clz\n");
        app.focus = Focus::Left;
        app.left_state.select(Some(0));

        yank_tree_node(&mut app, &list);
        assert!(matches!(app.clipboard, Some(ClipboardItem::Section(_))));

        app.left_state.select(Some(1));
        app.tree_items = build_tree_items(&list, &app.mode);
        paste_clipboard(&mut app, &mut list, f.path(), true).unwrap();

        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 3);
        assert_eq!(parsed.sections[0].name, "main");
        assert_eq!(parsed.sections[1].name, "main");
        assert_eq!(parsed.sections[2].name, "clz");
    }

    #[test]
    fn test_a_on_top_level_creates_child() {
        let (f, mut app, mut list) = setup("# main\n");
        app.focus = Focus::Left;
        app.left_state.select(Some(0));

        handle_normal(&mut app, &mut list, f.path(), KeyCode::Char('a')).unwrap();
        match &app.mode {
            Mode::InputSection {
                node: None, parent, ..
            } => {
                assert_eq!(parent.as_deref(), Some([0].as_slice()));
            }
            other => panic!("expected InputSection, got {:?}", other),
        }
    }

    #[test]
    fn test_capital_a_on_nested_creates_parent_level_above_subtree() {
        let (f, mut app, mut list) = setup("# main\n## sub\n");
        app.focus = Focus::Left;
        app.tree_items = build_tree_items(&list, &app.mode);
        let sub_pos = app
            .tree_items
            .iter()
            .position(|n| matches!(n, TreeItem::Node(p) if p == &vec![0, 0]))
            .unwrap();
        app.left_state.select(Some(sub_pos));

        handle_normal(&mut app, &mut list, f.path(), KeyCode::Char('A')).unwrap();
        match &app.mode {
            Mode::InputSection {
                node: None,
                parent,
                insert_idx,
                ..
            } => {
                assert_eq!(*parent, None);
                assert_eq!(*insert_idx, Some(0));
            }
            other => panic!("expected InputSection, got {:?}", other),
        }
    }

    #[test]
    fn test_capital_a_on_top_level_inserts_above() {
        let (f, mut app, mut list) = setup("# main\n# other\n");
        app.focus = Focus::Left;
        app.left_state.select(Some(1));

        handle_normal(&mut app, &mut list, f.path(), KeyCode::Char('A')).unwrap();
        match &app.mode {
            Mode::InputSection {
                node: None,
                parent,
                insert_idx,
                ..
            } => {
                assert_eq!(*parent, None);
                assert_eq!(*insert_idx, Some(1));
            }
            other => panic!("expected InputSection, got {:?}", other),
        }
    }

    #[test]
    fn test_o_creates_sibling_below() {
        let (f, mut app, mut list) = setup("# main\n# other\n");
        app.focus = Focus::Left;
        app.left_state.select(Some(0));

        handle_normal(&mut app, &mut list, f.path(), KeyCode::Char('o')).unwrap();
        match &app.mode {
            Mode::InputSection {
                node: None,
                parent,
                insert_idx,
                ..
            } => {
                assert_eq!(*parent, None);
                assert_eq!(*insert_idx, Some(1));
            }
            other => panic!("expected InputSection, got {:?}", other),
        }
    }

    #[test]
    fn test_capital_o_creates_sibling_above() {
        let (f, mut app, mut list) = setup("# main\n# other\n");
        app.focus = Focus::Left;
        app.left_state.select(Some(1));

        handle_normal(&mut app, &mut list, f.path(), KeyCode::Char('O')).unwrap();
        match &app.mode {
            Mode::InputSection {
                node: None,
                parent,
                insert_idx,
                ..
            } => {
                assert_eq!(*parent, None);
                assert_eq!(*insert_idx, Some(1));
            }
            other => panic!("expected InputSection, got {:?}", other),
        }
    }

    #[test]
    fn test_colon_sorts_tasks_by_due_date() {
        let content = "# main\n- [ ] C task due:2025-12-01\n- [ ] A task due:2025-01-15\n- [ ] B task\n";
        let (f, mut app, mut list) = setup(content);
        app.focus = Focus::Right;
        app.left_state.select(Some(0));
        app.right_state.select(Some(0));

        handle_normal(&mut app, &mut list, f.path(), KeyCode::Char(':')).unwrap();

        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks[0].text, "A task");
        assert_eq!(
            parsed.sections[0].tasks[0].due,
            Some(chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap())
        );
        assert_eq!(parsed.sections[0].tasks[1].text, "C task");
        assert_eq!(parsed.sections[0].tasks[2].text, "B task");
        assert!(parsed.sections[0].tasks[2].due.is_none());
    }
}
