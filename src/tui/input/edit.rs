use crate::parser::{resolve_relative_date, write_file};
use crate::task::{NodePath, Section, Task, TodoList};
use crate::tui::state::{AppState, Focus, Mode};
use crate::tui::{
    build_tree_items, get_task_from_ref_mut, get_task_refs, insert_section, rebuild_and_select,
    selected_node,
};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::path::Path;

fn char_to_byte(buf: &str, char_idx: usize) -> usize {
    buf.char_indices()
        .nth(char_idx)
        .map_or(buf.len(), |(b, _)| b)
}

fn insert_at_cursor(buf: &mut String, cursor: &mut usize, c: char) {
    let byte = char_to_byte(buf, *cursor);
    buf.insert(byte, c);
    *cursor += 1;
}

fn backspace_at_cursor(buf: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let start = char_to_byte(buf, *cursor - 1);
    let end = char_to_byte(buf, *cursor);
    buf.replace_range(start..end, "");
    *cursor -= 1;
}

fn delete_at_cursor(buf: &mut String, cursor: usize) {
    if cursor >= buf.chars().count() {
        return;
    }
    let start = char_to_byte(buf, cursor);
    let end = char_to_byte(buf, cursor + 1);
    buf.replace_range(start..end, "");
}

const fn move_left(cursor: &mut usize) {
    *cursor = cursor.saturating_sub(1);
}

fn move_right(cursor: &mut usize, buf: &str) {
    let max = buf.chars().count();
    if *cursor < max {
        *cursor += 1;
    }
}

pub struct InputTaskParams {
    pub editing_idx: Option<usize>,
    pub insert_idx: Option<usize>,
    pub above: bool,
    pub buf: String,
    pub cursor: usize,
}

pub fn handle_input_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    params: &mut InputTaskParams,
) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            commit_input_task(app, todo_list, path, params)?;
            app.mode = Mode::Normal;
        }
        KeyCode::Char(c) => {
            insert_at_cursor(&mut params.buf, &mut params.cursor, c);
        }
        KeyCode::Backspace => {
            backspace_at_cursor(&mut params.buf, &mut params.cursor);
        }
        KeyCode::Delete => {
            delete_at_cursor(&mut params.buf, params.cursor);
        }
        KeyCode::Left => {
            move_left(&mut params.cursor);
        }
        KeyCode::Right => {
            move_right(&mut params.cursor, &params.buf);
        }
        KeyCode::Home => {
            params.cursor = 0;
        }
        KeyCode::End => {
            params.cursor = params.buf.chars().count();
        }
        _ => {}
    }

    if matches!(app.mode, Mode::InputTask { .. }) {
        app.mode = Mode::InputTask {
            editing_idx: params.editing_idx,
            insert_idx: params.insert_idx,
            above: params.above,
            buf: params.buf.clone(),
            cursor: params.cursor,
        };
    }
    Ok(())
}

fn commit_input_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    params: &InputTaskParams,
) -> Result<()> {
    let InputTaskParams {
        editing_idx,
        insert_idx,
        above,
        buf,
        ..
    } = params;

    let text = buf.trim().to_string();
    if !text.is_empty()
        && let Some(node) = selected_node(app)
    {
        match editing_idx {
            None => {
                let refs = get_task_refs(todo_list, &node);

                let anchor =
                    insert_idx.map_or(refs.len(), |p| if *above { p } else { p.saturating_sub(1) });

                let (target_node, ins_idx) = refs.get(anchor).map_or_else(
                    || {
                        let n = node_task_count(todo_list, &node);
                        (node.clone(), n)
                    },
                    |r| {
                        let base = r.task_idx;
                        (r.node.clone(), if *above { base } else { base + 1 })
                    },
                );

                if let Some(sec) = crate::task::get_node_mut(todo_list, &target_node) {
                    sec.tasks.insert(
                        ins_idx,
                        Task {
                            text,
                            is_done: false,
                            due: None,
                        },
                    );
                }
                write_file(path, todo_list)?;
                app.tree_items = build_tree_items(todo_list, &app.mode);
                let new_refs = get_task_refs(todo_list, &node);
                if let Some(pos) = new_refs
                    .iter()
                    .position(|r| r.node == target_node && r.task_idx == ins_idx)
                {
                    app.right_state.select(Some(pos));
                } else {
                    app.right_state.select(Some(
                        insert_idx.unwrap_or(new_refs.len()).min(new_refs.len()),
                    ));
                }
                app.focus = Focus::Right;
            }
            Some(idx) => {
                let refs = get_task_refs(todo_list, &node);
                if let Some(ref_item) = refs.get(*idx) {
                    let task = get_task_from_ref_mut(todo_list, ref_item);
                    task.text = text;
                    write_file(path, todo_list)?;
                }
            }
        }
    }
    Ok(())
}

pub struct InputDueParams {
    pub task_idx: usize,
    pub buf: String,
    pub cursor: usize,
}

pub fn handle_input_due(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    params: &mut InputDueParams,
) -> Result<()> {
    let InputDueParams {
        task_idx,
        buf,
        cursor,
    } = params;

    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            if let Some(node) = selected_node(app) {
                let refs = get_task_refs(todo_list, &node);
                if let Some(ref_item) = refs.get(*task_idx) {
                    let task = get_task_from_ref_mut(todo_list, ref_item);
                    let trimmed = buf.trim();
                    task.due = if trimmed.is_empty() {
                        None
                    } else {
                        resolve_relative_date(trimmed)
                    };
                    write_file(path, todo_list)?;
                }
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char(c) => {
            insert_at_cursor(buf, cursor, c);
        }
        KeyCode::Backspace => {
            backspace_at_cursor(buf, cursor);
        }
        KeyCode::Delete => {
            delete_at_cursor(buf, *cursor);
        }
        KeyCode::Left => {
            move_left(cursor);
        }
        KeyCode::Right => {
            move_right(cursor, buf);
        }
        KeyCode::Home => {
            *cursor = 0;
        }
        KeyCode::End => {
            *cursor = buf.chars().count();
        }
        _ => {}
    }

    if matches!(app.mode, Mode::InputDue { .. }) {
        app.mode = Mode::InputDue {
            task_idx: *task_idx,
            buf: buf.clone(),
            cursor: *cursor,
        };
    }
    Ok(())
}

pub struct InputSectionParams {
    pub node: Option<NodePath>,
    pub parent: Option<NodePath>,
    pub insert_idx: Option<usize>,
    pub buf: String,
    pub cursor: usize,
}

pub fn handle_input_section(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    params: &mut InputSectionParams,
) -> Result<()> {
    let InputSectionParams {
        node,
        parent,
        insert_idx,
        buf,
        cursor,
    } = params;

    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            backspace_at_cursor(buf, cursor);
        }
        KeyCode::Delete => {
            delete_at_cursor(buf, *cursor);
        }
        KeyCode::Left => {
            move_left(cursor);
        }
        KeyCode::Right => {
            move_right(cursor, buf);
        }
        KeyCode::Home => {
            *cursor = 0;
        }
        KeyCode::End => {
            *cursor = buf.chars().count();
        }
        KeyCode::Enter => {
            let name = buf.trim().to_string();
            if !name.is_empty() {
                match node {
                    None => {
                        let new_path = insert_section(
                            todo_list,
                            parent.as_ref(),
                            insert_idx.unwrap_or(usize::MAX),
                            Section::new(name),
                        );
                        write_file(path, todo_list)?;
                        app.mode = Mode::Normal;
                        rebuild_and_select(app, todo_list, &new_path);
                        app.right_state.select(Some(0));
                    }
                    Some(existing) => {
                        if let Some(sec) = crate::task::get_node_mut(todo_list, existing) {
                            sec.name = name;
                            write_file(path, todo_list)?;
                        }
                        app.mode = Mode::Normal;
                        rebuild_and_select(app, todo_list, existing);
                    }
                }
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char(c) => {
            insert_at_cursor(buf, cursor, c);
        }
        _ => {}
    }

    if matches!(app.mode, Mode::InputSection { .. }) {
        app.mode = Mode::InputSection {
            node: node.clone(),
            parent: parent.clone(),
            insert_idx: *insert_idx,
            buf: buf.clone(),
            cursor: *cursor,
        };
    }
    Ok(())
}

fn node_task_count(todo_list: &TodoList, node: &NodePath) -> usize {
    crate::task::get_node(todo_list, node).map_or(0, |s| s.tasks.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;
    use crate::tui::state::{AppState, Mode};
    use chrono::NaiveDate;
    use crossterm::event::KeyCode;
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
    fn input_task_esc_returns_to_normal() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "typed".to_string(),
            cursor: 5,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Esc, &mut params).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn input_task_backspace_pops_char() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "hello".to_string(),
            cursor: 5,
        };
        handle_input_task(
            &mut app,
            &mut list,
            f.path(),
            KeyCode::Backspace,
            &mut params,
        )
        .unwrap();
        assert_eq!(params.buf, "hell");
        assert_eq!(params.cursor, 4);
    }

    #[test]
    fn input_task_backspace_at_start_is_noop() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "hello".to_string(),
            cursor: 0,
        };
        handle_input_task(
            &mut app,
            &mut list,
            f.path(),
            KeyCode::Backspace,
            &mut params,
        )
        .unwrap();
        assert_eq!(params.buf, "hello");
        assert_eq!(params.cursor, 0);
    }

    #[test]
    fn input_task_left_then_type_inserts_in_middle() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "ac".to_string(),
            cursor: 2,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Left, &mut params).unwrap();
        handle_input_task(
            &mut app,
            &mut list,
            f.path(),
            KeyCode::Char('b'),
            &mut params,
        )
        .unwrap();
        assert_eq!(params.buf, "abc");
        assert_eq!(params.cursor, 2);
    }

    #[test]
    fn input_task_right_at_end_is_noop() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "abc".to_string(),
            cursor: 3,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Right, &mut params).unwrap();
        assert_eq!(params.buf, "abc");
        assert_eq!(params.cursor, 3);
    }

    #[test]
    fn input_task_home_and_end_move_cursor() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "hello".to_string(),
            cursor: 5,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Home, &mut params).unwrap();
        assert_eq!(params.cursor, 0);
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::End, &mut params).unwrap();
        assert_eq!(params.cursor, 5);
    }

    #[test]
    fn input_task_delete_removes_char_after_cursor() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "abc".to_string(),
            cursor: 1,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Delete, &mut params).unwrap();
        assert_eq!(params.buf, "ac");
        assert_eq!(params.cursor, 1);
    }

    #[test]
    fn input_task_char_appends_to_buf() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "hel".to_string(),
            cursor: 3,
        };
        handle_input_task(
            &mut app,
            &mut list,
            f.path(),
            KeyCode::Char('l'),
            &mut params,
        )
        .unwrap();
        assert_eq!(params.buf, "hell");
        assert_eq!(params.cursor, 4);
    }

    #[test]
    fn input_task_enter_adds_new_task() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: Some(1),
            above: false,
            buf: "New task".to_string(),
            cursor: 8,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks.len(), 2);
        assert_eq!(parsed.sections[0].tasks[1].text, "New task");
    }

    #[test]
    fn input_task_enter_empty_buf_does_not_add() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "   ".to_string(),
            cursor: 3,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks.len(), 1);
    }

    #[test]
    fn input_task_enter_edits_existing_task() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Old text\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));
        let mut params = InputTaskParams {
            editing_idx: Some(0),
            insert_idx: None,
            above: false,
            buf: "Updated text".to_string(),
            cursor: 12,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks[0].text, "Updated text");
    }

    #[test]
    fn input_task_enter_appends_to_nested_node() {
        let (f, mut app, mut list) = setup("# main\n## sub\n- [ ] sub task\n");
        app.focus = Focus::Right;
        app.tree_items = build_tree_items(&list, &app.mode);
        let pos = app
            .tree_items
            .iter()
            .position(|n| matches!(n, crate::tui::state::TreeItem::Node(p) if p == &vec![0, 0]))
            .unwrap();
        app.left_state.select(Some(pos));
        app.right_state.select(Some(0));
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: None,
            above: false,
            buf: "Another".to_string(),
            cursor: 7,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].children[0].tasks.len(), 2);
        assert_eq!(parsed.sections[0].children[0].tasks[1].text, "Another");
    }

    #[test]
    fn input_task_a_below_last_root_task_stays_in_root() {
        let (f, mut app, mut list) = setup("# main\n- [ ] A\n- [ ] B\n## sub\n- [ ] C\n");
        app.focus = Focus::Right;
        app.left_state.select(Some(0));
        app.right_state.select(Some(1));
        let mut params = InputTaskParams {
            editing_idx: None,
            insert_idx: Some(2),
            above: false,
            buf: "New".to_string(),
            cursor: 3,
        };
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();

        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks.len(), 3);
        assert_eq!(parsed.sections[0].tasks[2].text, "New");
        assert_eq!(parsed.sections[0].children[0].tasks.len(), 1);
        assert_eq!(parsed.sections[0].children[0].tasks[0].text, "C");
    }

    #[test]
    fn input_due_enter_valid_date_sets_due() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));
        let mut params = InputDueParams {
            task_idx: 0,
            buf: "2025-06-15".to_string(),
            cursor: 10,
        };
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(
            parsed.sections[0].tasks[0].due,
            Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap())
        );
    }

    #[test]
    fn input_due_left_then_type_inserts() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));
        let mut params = InputDueParams {
            task_idx: 0,
            buf: "2025".to_string(),
            cursor: 4,
        };
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Left, &mut params).unwrap();
        handle_input_due(
            &mut app,
            &mut list,
            f.path(),
            KeyCode::Char('-'),
            &mut params,
        )
        .unwrap();
        assert_eq!(params.buf, "202-5");
        assert_eq!(params.cursor, 4);
    }

    #[test]
    fn input_due_enter_empty_clears_due() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task due:2025-01-01\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));
        let mut params = InputDueParams {
            task_idx: 0,
            buf: String::new(),
            cursor: 0,
        };
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert!(parsed.sections[0].tasks[0].due.is_none());
    }

    #[test]
    fn input_due_enter_invalid_sets_none() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));
        let mut params = InputDueParams {
            task_idx: 0,
            buf: "not-a-date".to_string(),
            cursor: 10,
        };
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert!(parsed.sections[0].tasks[0].due.is_none());
    }

    #[test]
    fn input_due_esc_cancels() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task\n");
        let mut params = InputDueParams {
            task_idx: 0,
            buf: "2025".to_string(),
            cursor: 4,
        };
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Esc, &mut params).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn input_section_enter_creates_new_section() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut params = InputSectionParams {
            node: None,
            parent: None,
            insert_idx: None,
            buf: "work".to_string(),
            cursor: 4,
        };
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 2);
        assert_eq!(parsed.sections[1].name, "work");
    }

    #[test]
    fn input_section_left_then_type_inserts() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut params = InputSectionParams {
            node: None,
            parent: None,
            insert_idx: None,
            buf: "end".to_string(),
            cursor: 1,
        };
        handle_input_section(
            &mut app,
            &mut list,
            f.path(),
            KeyCode::Char('m'),
            &mut params,
        )
        .unwrap();
        assert_eq!(params.buf, "emnd");
        assert_eq!(params.cursor, 2);
    }

    #[test]
    fn input_section_enter_renames_existing() {
        let (f, mut app, mut list) = setup("# old_name\n");
        let mut params = InputSectionParams {
            node: Some(vec![0]),
            parent: None,
            insert_idx: None,
            buf: "new_name".to_string(),
            cursor: 8,
        };
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].name, "new_name");
    }

    #[test]
    fn input_section_enter_with_insert_idx() {
        let (f, mut app, mut list) = setup("# first\n# third\n");
        let mut params = InputSectionParams {
            node: None,
            parent: None,
            insert_idx: Some(1),
            buf: "second".to_string(),
            cursor: 6,
        };
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 3);
        assert_eq!(parsed.sections[0].name, "first");
        assert_eq!(parsed.sections[1].name, "second");
        assert_eq!(parsed.sections[2].name, "third");
    }

    #[test]
    fn input_section_creates_nested_child() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut params = InputSectionParams {
            node: None,
            parent: Some(vec![0]),
            insert_idx: Some(0),
            buf: "child".to_string(),
            cursor: 5,
        };
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].name, "main");
        assert_eq!(parsed.sections[0].children.len(), 1);
        assert_eq!(parsed.sections[0].children[0].name, "child");
    }

    #[test]
    fn input_section_esc_cancels() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut params = InputSectionParams {
            node: None,
            parent: None,
            insert_idx: None,
            buf: "partial".to_string(),
            cursor: 7,
        };
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Esc, &mut params).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn input_section_empty_name_does_not_create() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut params = InputSectionParams {
            node: None,
            parent: None,
            insert_idx: None,
            buf: "   ".to_string(),
            cursor: 3,
        };
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, &mut params).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 1);
    }
}
