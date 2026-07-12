use crate::parser::{resolve_relative_date, write_file};
use crate::task::{Section, Task, TodoList};
use crate::tui::state::{AppState, Focus, Mode, TreeNode};
use crate::tui::{get_task_from_ref_mut, get_task_refs, selected_node};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::path::Path;

fn char_to_byte(buf: &str, char_idx: usize) -> usize {
    buf.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(buf.len())
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

fn delete_at_cursor(buf: &mut String, cursor: &mut usize) {
    if *cursor >= buf.chars().count() {
        return;
    }
    let start = char_to_byte(buf, *cursor);
    let end = char_to_byte(buf, *cursor + 1);
    buf.replace_range(start..end, "");
}

fn move_left(cursor: &mut usize) {
    *cursor = cursor.saturating_sub(1);
}

fn move_right(cursor: &mut usize, buf: &str) {
    let max = buf.chars().count();
    if *cursor < max {
        *cursor += 1;
    }
}

pub fn handle_input_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    editing_idx: Option<usize>,
    insert_idx: Option<usize>,
    buf: &mut String,
    cursor: &mut usize,
) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            let text = buf.trim().to_string();
            if !text.is_empty()
                && let Some(node) = selected_node(app)
            {
                match editing_idx {
                    None => {
                        match node {
                            TreeNode::Section(s) => {
                                let sec = &mut todo_list.sections[s];
                                let new_task = Task {
                                    text,
                                    is_done: false,
                                    due: None,
                                };
                                if let Some(i) = insert_idx {
                                    let i = i.min(sec.tasks.len());
                                    sec.tasks.insert(i, new_task);
                                } else {
                                    sec.tasks.push(new_task);
                                }
                            }
                            TreeNode::Subsection(s, sb) => {
                                let sub = &mut todo_list.sections[s].subsections[sb];
                                let new_task = Task {
                                    text,
                                    is_done: false,
                                    due: None,
                                };
                                if let Some(i) = insert_idx {
                                    let i = i.min(sub.tasks.len());
                                    sub.tasks.insert(i, new_task);
                                } else {
                                    sub.tasks.push(new_task);
                                }
                            }
                        }
                        write_file(path, todo_list)?;
                        let new_len = get_task_refs(todo_list, node).len();
                        app.right_state.select(Some(new_len.saturating_sub(1)));
                    }
                    Some(idx) => {
                        let refs = get_task_refs(todo_list, node);
                        if let Some(ref_item) = refs.get(idx) {
                            let task = get_task_from_ref_mut(todo_list, ref_item);
                            task.text = text;
                            write_file(path, todo_list)?;
                        }
                    }
                }
            }
            app.mode = Mode::Normal;

            if editing_idx.is_none() {
                app.focus = Focus::Right;
            }
        }
        KeyCode::Char(c) => {
            insert_at_cursor(buf, cursor, c);
        }
        KeyCode::Backspace => {
            backspace_at_cursor(buf, cursor);
        }
        KeyCode::Delete => {
            delete_at_cursor(buf, cursor);
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

    if matches!(app.mode, Mode::InputTask { .. }) {
        app.mode = Mode::InputTask {
            editing_idx,
            insert_idx,
            buf: buf.clone(),
            cursor: *cursor,
        };
    }
    Ok(())
}

pub fn handle_input_due(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    task_idx: usize,
    buf: &mut String,
    cursor: &mut usize,
) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            if let Some(node) = selected_node(app) {
                let refs = get_task_refs(todo_list, node);
                if let Some(ref_item) = refs.get(task_idx) {
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
            delete_at_cursor(buf, cursor);
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
            task_idx,
            buf: buf.clone(),
            cursor: *cursor,
        };
    }
    Ok(())
}

pub fn handle_input_section(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    node: Option<TreeNode>,
    insert_idx: Option<usize>,
    buf: &mut String,
    cursor: &mut usize,
) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            backspace_at_cursor(buf, cursor);
        }
        KeyCode::Delete => {
            delete_at_cursor(buf, cursor);
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
                        let new_sec = Section {
                            name,
                            tasks: Vec::new(),
                            subsections: Vec::new(),
                        };
                        let new_s = if let Some(i) = insert_idx {
                            let i = i.min(todo_list.sections.len());
                            todo_list.sections.insert(i, new_sec);
                            i
                        } else {
                            todo_list.sections.push(new_sec);
                            todo_list.sections.len() - 1
                        };
                        write_file(path, todo_list)?;

                        app.mode = Mode::Normal;
                        let temp = crate::tui::build_tree_nodes(todo_list, &app.mode);
                        let tree_pos = temp
                            .iter()
                            .position(|n| matches!(n, TreeNode::Section(s) if *s == new_s));
                        if let Some(pos) = tree_pos {
                            app.left_state.select(Some(pos));
                        }
                    }

                    Some(TreeNode::Section(s)) => {
                        if s < todo_list.sections.len() {
                            todo_list.sections[s].name = name;
                            write_file(path, todo_list)?;
                        }
                    }

                    Some(TreeNode::Subsection(s, sb)) => {
                        if s < todo_list.sections.len()
                            && sb < todo_list.sections[s].subsections.len()
                        {
                            todo_list.sections[s].subsections[sb].name = name;
                            write_file(path, todo_list)?;
                        }
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
            node,
            insert_idx,
            buf: buf.clone(),
            cursor: *cursor,
        };
    }
    Ok(())
}

pub fn handle_input_subsection(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    parent_sec_idx: usize,
    insert_idx: Option<usize>,
    buf: &mut String,
    cursor: &mut usize,
) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            backspace_at_cursor(buf, cursor);
        }
        KeyCode::Delete => {
            delete_at_cursor(buf, cursor);
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
            if !name.is_empty() && parent_sec_idx < todo_list.sections.len() {
                let new_sub = crate::task::Subsection {
                    name,
                    tasks: Vec::new(),
                };
                let new_sub_idx = if let Some(i) = insert_idx {
                    let i = i.min(todo_list.sections[parent_sec_idx].subsections.len());
                    todo_list.sections[parent_sec_idx]
                        .subsections
                        .insert(i, new_sub);
                    i
                } else {
                    todo_list.sections[parent_sec_idx].subsections.push(new_sub);
                    todo_list.sections[parent_sec_idx].subsections.len() - 1
                };
                write_file(path, todo_list)?;

                app.mode = Mode::Normal;
                let temp_tree = crate::tui::build_tree_nodes(todo_list, &app.mode);
                let target_node = TreeNode::Subsection(parent_sec_idx, new_sub_idx);
                if let Some(pos) = temp_tree.iter().position(|n| *n == target_node) {
                    app.left_state.select(Some(pos));
                }
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char(c) => {
            insert_at_cursor(buf, cursor, c);
        }
        _ => {}
    }

    if matches!(app.mode, Mode::InputSubsection { .. }) {
        app.mode = Mode::InputSubsection {
            parent_sec_idx,
            insert_idx,
            buf: buf.clone(),
            cursor: *cursor,
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;
    use crate::task::TodoList;
    use crate::tui::build_tree_nodes;
    use crate::tui::state::{AppState, Focus, Mode, TreeNode};
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
        app.tree_nodes = build_tree_nodes(&list, &app.mode);
        (f, app, list)
    }

    #[test]
    fn input_task_esc_returns_to_normal() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "typed".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Esc, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn input_task_backspace_pops_char() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "hello".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Backspace, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "hell");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn input_task_backspace_at_start_is_noop() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "hello".to_string();
        let mut cursor = 0;
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Backspace, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "hello");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn input_task_left_then_type_inserts_in_middle() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "ac".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Left, None, None, &mut buf, &mut cursor).unwrap();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Char('b'), None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "abc");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn input_task_right_at_end_is_noop() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "abc".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Right, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "abc");
        assert_eq!(cursor, 3);
    }

    #[test]
    fn input_task_home_and_end_move_cursor() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "hello".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Home, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(cursor, 0);
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::End, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(cursor, 5);
    }

    #[test]
    fn input_task_delete_removes_char_after_cursor() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "abc".to_string();
        let mut cursor = 1;
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Delete, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "ac");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn input_task_char_appends_to_buf() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "hel".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Char('l'), None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "hell");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn input_task_enter_adds_new_task() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        app.focus = Focus::Right;
        let mut buf = "New task".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Enter, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks.len(), 2);
        assert_eq!(parsed.sections[0].tasks[1].text, "New task");
    }

    #[test]
    fn input_task_enter_empty_buf_does_not_add() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        let mut buf = "   ".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Enter, None, None, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks.len(), 1);
    }

    #[test]
    fn input_task_enter_edits_existing_task() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Old text\n");
        app.focus = Focus::Right;
        let mut buf = "Updated text".to_string();
        let mut cursor = buf.chars().count();
        handle_input_task(&mut app, &mut list, f.path(), KeyCode::Enter, Some(0), None, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks[0].text, "Updated text");
    }

    #[test]
    fn input_due_enter_valid_date_sets_due() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task\n");
        app.focus = Focus::Right;
        let mut buf = "2025-06-15".to_string();
        let mut cursor = buf.chars().count();
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Enter, 0, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].tasks[0].due, Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap()));
    }

    #[test]
    fn input_due_left_then_type_inserts() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task\n");
        app.focus = Focus::Right;
        let mut buf = "2025".to_string();
        let mut cursor = buf.chars().count();
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Left, 0, &mut buf, &mut cursor).unwrap();
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Char('-'), 0, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "202-5");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn input_due_enter_empty_clears_due() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task due:2025-01-01\n");
        app.focus = Focus::Right;
        let mut buf = String::new();
        let mut cursor = 0;
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Enter, 0, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert!(parsed.sections[0].tasks[0].due.is_none());
    }

    #[test]
    fn input_due_enter_invalid_sets_none() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task\n");
        app.focus = Focus::Right;
        let mut buf = "not-a-date".to_string();
        let mut cursor = buf.chars().count();
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Enter, 0, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert!(parsed.sections[0].tasks[0].due.is_none());
    }

    #[test]
    fn input_due_esc_cancels() {
        let (f, mut app, mut list) = setup("# main\n- [ ] Task\n");
        let mut buf = "2025".to_string();
        let mut cursor = buf.chars().count();
        handle_input_due(&mut app, &mut list, f.path(), KeyCode::Esc, 0, &mut buf, &mut cursor).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn input_section_enter_creates_new_section() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut buf = "work".to_string();
        let mut cursor = buf.chars().count();
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, None, None, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 2);
        assert_eq!(parsed.sections[1].name, "work");
    }

    #[test]
    fn input_section_left_then_type_inserts() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut buf = "end".to_string();
        let mut cursor = 1;
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Char('m'), None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "emnd");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn input_section_enter_renames_existing() {
        let (f, mut app, mut list) = setup("# old_name\n");
        let mut buf = "new_name".to_string();
        let mut cursor = buf.chars().count();
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, Some(TreeNode::Section(0)), None, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].name, "new_name");
    }

    #[test]
    fn input_section_enter_with_insert_idx() {
        let (f, mut app, mut list) = setup("# first\n# third\n");
        let mut buf = "second".to_string();
        let mut cursor = buf.chars().count();
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, None, Some(1), &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 3);
        assert_eq!(parsed.sections[0].name, "first");
        assert_eq!(parsed.sections[1].name, "second");
        assert_eq!(parsed.sections[2].name, "third");
    }

    #[test]
    fn input_section_esc_cancels() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut buf = "partial".to_string();
        let mut cursor = buf.chars().count();
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Esc, None, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn input_section_empty_name_does_not_create() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut buf = "   ".to_string();
        let mut cursor = buf.chars().count();
        handle_input_section(&mut app, &mut list, f.path(), KeyCode::Enter, None, None, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections.len(), 1);
    }

    #[test]
    fn input_subsection_enter_creates_subsection() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut buf = "urgent".to_string();
        let mut cursor = buf.chars().count();
        handle_input_subsection(&mut app, &mut list, f.path(), KeyCode::Enter, 0, None, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert_eq!(parsed.sections[0].subsections.len(), 1);
        assert_eq!(parsed.sections[0].subsections[0].name, "urgent");
    }

    #[test]
    fn input_subsection_left_then_type_inserts() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut buf = "urgent".to_string();
        let mut cursor = 3;
        handle_input_subsection(&mut app, &mut list, f.path(), KeyCode::Char('-'), 0, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(buf, "urg-ent");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn input_subsection_esc_cancels() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut buf = "partial".to_string();
        let mut cursor = buf.chars().count();
        handle_input_subsection(&mut app, &mut list, f.path(), KeyCode::Esc, 0, None, &mut buf, &mut cursor).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn input_subsection_empty_name_does_not_create() {
        let (f, mut app, mut list) = setup("# main\n");
        let mut buf = "  ".to_string();
        let mut cursor = buf.chars().count();
        handle_input_subsection(&mut app, &mut list, f.path(), KeyCode::Enter, 0, None, &mut buf, &mut cursor).unwrap();
        let parsed = parse_file(f.path()).unwrap();
        assert!(parsed.sections[0].subsections.is_empty());
    }
}
