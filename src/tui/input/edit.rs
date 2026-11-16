//! Input-mode key handling and commits.
//!
//! The three input modes share the same text editor from [`super::text`]. Only
//! Enter commits changes; Esc leaves the document untouched.

use crate::due::{extract_inline_due, resolve_relative_date};
use crate::parser::write_file;
use crate::task::{Section, Task, TodoList, get_node_mut, insert_section, task_count};
use crate::tui::input::text;
use crate::tui::state::{AppState, DueInput, Focus, Mode, SectionInput, TaskInput};
use crate::tui::task_ref::{get_task_from_ref_mut, get_task_refs_filtered};
use crate::tui::tree::rebuild_and_select;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::Path;

/// Handle a key while creating or renaming a task.
pub fn handle_input_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            return Ok(());
        }
        KeyCode::Enter => {
            let input = match &app.mode {
                Mode::InputTask(input) => input.clone(),
                _ => return Ok(()),
            };
            commit_task(app, todo_list, path, &input)?;
            app.mode = Mode::Normal;
            return Ok(());
        }
        _ => {}
    }

    if let Mode::InputTask(input) = &mut app.mode {
        text::handle_key(&mut input.field, code, modifiers);
    }
    Ok(())
}

fn commit_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    input: &TaskInput,
) -> Result<()> {
    let raw = input.field.buf.trim();
    if raw.is_empty() {
        return Ok(());
    }
    let (text, due) = extract_inline_due(raw);

    match input.editing_idx {
        None => insert_task(app, todo_list, path, input, text, due),
        Some(editing_idx) => rename_task(app, todo_list, path, input, editing_idx, text, due),
    }
}

fn insert_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    input: &TaskInput,
    text: String,
    due: Option<chrono::NaiveDate>,
) -> Result<()> {
    let refs = get_task_refs_filtered(todo_list, &input.node, &app.filter_lower);
    let anchor = input.insert_idx.map_or(refs.len(), |position| {
        if input.above {
            position
        } else {
            position.saturating_sub(1)
        }
    });
    let (target_node, insert_index) = refs.get(anchor).map_or_else(
        || (input.node.clone(), task_count(todo_list, &input.node)),
        |reference| {
            let base = reference.task_idx;
            (
                reference.node.clone(),
                if input.above { base } else { base + 1 },
            )
        },
    );

    let Some(section) = get_node_mut(todo_list, &target_node) else {
        return Ok(());
    };
    let mut task = Task::new(text);
    task.due = due;
    section.tasks.insert(insert_index, task);
    write_file(path, todo_list)?;

    app.focus = Focus::Right;
    select_inserted_task(app, todo_list, &target_node, insert_index, input.insert_idx);
    Ok(())
}

fn select_inserted_task(
    app: &mut AppState,
    todo_list: &TodoList,
    target_node: &[usize],
    insert_index: usize,
    requested_row: Option<usize>,
) {
    let node = target_node.to_vec();
    let new_refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower);
    if let Some(position) = new_refs.iter().position(|reference| {
        reference.node.as_slice() == target_node && reference.task_idx == insert_index
    }) {
        app.right_state.select(Some(position));
    } else if new_refs.is_empty() {
        app.right_state.select(None);
    } else {
        let row = requested_row
            .unwrap_or(new_refs.len())
            .min(new_refs.len() - 1);
        app.right_state.select(Some(row));
    }
}

fn rename_task(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    input: &TaskInput,
    editing_idx: usize,
    text: String,
    due: Option<chrono::NaiveDate>,
) -> Result<()> {
    let refs = get_task_refs_filtered(todo_list, &input.node, &app.filter_lower);
    let Some(reference) = refs.get(editing_idx) else {
        return Ok(());
    };
    let Some(task) = get_task_from_ref_mut(todo_list, reference) else {
        return Ok(());
    };

    task.text = text;
    if due.is_some() {
        task.due = due;
    }
    write_file(path, todo_list)
}

/// Handle a key while editing a due date.
pub fn handle_input_due(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            return Ok(());
        }
        KeyCode::Enter => {
            let input = match &app.mode {
                Mode::InputDue(input) => input.clone(),
                _ => return Ok(()),
            };
            commit_due(todo_list, path, &input)?;
            app.mode = Mode::Normal;
            return Ok(());
        }
        _ => {}
    }

    if let Mode::InputDue(input) = &mut app.mode {
        text::handle_key(&mut input.field, code, modifiers);
    }
    Ok(())
}

fn commit_due(todo_list: &mut TodoList, path: &Path, input: &DueInput) -> Result<()> {
    let trimmed = input.field.buf.trim();
    let due = if trimmed.is_empty() {
        None
    } else {
        resolve_relative_date(trimmed)
    };

    let Some(task) = get_task_from_ref_mut(todo_list, &input.target) else {
        return Ok(());
    };
    task.due = due;
    write_file(path, todo_list)
}

/// Handle a key while creating or renaming a section.
pub fn handle_input_section(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Result<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            return Ok(());
        }
        KeyCode::Enter => {
            let input = match &app.mode {
                Mode::InputSection(input) => input.clone(),
                _ => return Ok(()),
            };
            commit_section(app, todo_list, path, &input)?;
            app.mode = Mode::Normal;
            return Ok(());
        }
        _ => {}
    }

    if let Mode::InputSection(input) = &mut app.mode {
        text::handle_key(&mut input.field, code, modifiers);
    }
    Ok(())
}

fn commit_section(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    input: &SectionInput,
) -> Result<()> {
    let name = input.field.buf.trim();
    if name.is_empty() {
        return Ok(());
    }

    match &input.node {
        None => {
            let section = Section::with_level(name, input.level);
            let Some(new_path) = insert_section(
                todo_list,
                input.parent.as_ref(),
                input.insert_idx.unwrap_or(usize::MAX),
                section,
                input.level,
            ) else {
                return Ok(());
            };
            write_file(path, todo_list)?;
            app.mode = Mode::Normal;
            rebuild_and_select(app, todo_list, &new_path);
            app.right_state.select(None);
        }
        Some(existing) => {
            if let Some(section) = get_node_mut(todo_list, existing) {
                section.name = name.to_string();
                write_file(path, todo_list)?;
            }
            app.mode = Mode::Normal;
            rebuild_and_select(app, todo_list, existing);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_str;
    use crate::task::TodoList;
    use crate::tui::state::{Mode, TextField};
    use crate::tui::task_ref::TaskRef;
    use crate::tui::tree::{build_tree_items, selected_node};
    use chrono::NaiveDate;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn setup(content: &str) -> (NamedTempFile, AppState, TodoList) {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        let list = parse_str(content);
        let mut app = AppState::new(list.sections.len());
        app.tree_items = build_tree_items(&list, &app.mode);
        (file, app, list)
    }

    fn task_input(
        node: Vec<usize>,
        editing_idx: Option<usize>,
        insert_idx: Option<usize>,
        text: &str,
    ) -> TaskInput {
        TaskInput {
            editing_idx,
            insert_idx,
            above: false,
            node,
            field: TextField::new(text),
        }
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn esc_cancels_each_input_mode_without_saving() {
        let (file, mut app, mut list) = setup("# main\n- [ ] Task\n");

        app.mode = Mode::InputTask(task_input(vec![0], None, None, "draft"));
        handle_input_task(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Esc,
            KeyModifiers::NONE,
        )
        .unwrap();
        assert_eq!(app.mode, Mode::Normal);

        app.mode = Mode::InputDue(DueInput {
            target: TaskRef {
                node: vec![0],
                task_idx: 0,
                sub_name: None,
            },
            field: TextField::new("2025-01-01"),
        });
        handle_input_due(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Esc,
            KeyModifiers::NONE,
        )
        .unwrap();
        assert_eq!(app.mode, Mode::Normal);

        app.mode = Mode::InputSection(SectionInput {
            node: None,
            parent: None,
            insert_idx: None,
            level: 1,
            field: TextField::new("draft"),
        });
        handle_input_section(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Esc,
            KeyModifiers::NONE,
        )
        .unwrap();
        assert_eq!(app.mode, Mode::Normal);

        assert_eq!(list, parse_str("# main\n- [ ] Task\n"));
    }

    #[test]
    fn text_editing_is_shared_and_unicode_safe() {
        let (file, mut app, mut list) = setup("# main\n");
        app.mode = Mode::InputTask(task_input(vec![0], None, None, "aé"));
        handle_input_task(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Char('日'),
            KeyModifiers::NONE,
        )
        .unwrap();
        handle_input_task(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Backspace,
            KeyModifiers::NONE,
        )
        .unwrap();

        match app.mode {
            Mode::InputTask(ref input) => assert_eq!(input.field.buf, "aé"),
            ref other => panic!("expected task input, got {other:?}"),
        }
    }

    #[test]
    fn enter_adds_task_with_inline_due() {
        let (file, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        app.focus = Focus::Right;
        app.right_state.select(Some(0));
        app.mode = Mode::InputTask(task_input(
            vec![0],
            None,
            Some(1),
            "New task due:2025-06-15",
        ));

        handle_input_task(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();

        let parsed = parse_str(&std::fs::read_to_string(file.path()).unwrap());
        assert_eq!(parsed.sections[0].tasks.len(), 2);
        assert_eq!(parsed.sections[0].tasks[1].text, "New task");
        assert_eq!(parsed.sections[0].tasks[1].due, Some(date(2025, 6, 15)));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Right);
    }

    #[test]
    fn enter_edits_existing_task_and_can_set_due() {
        let (file, mut app, mut list) = setup("# main\n- [ ] Old\n");
        app.mode = Mode::InputTask(task_input(vec![0], Some(0), None, "Updated due:2025-06-15"));

        handle_input_task(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();

        let parsed = parse_str(&std::fs::read_to_string(file.path()).unwrap());
        assert_eq!(parsed.sections[0].tasks[0].text, "Updated");
        assert_eq!(parsed.sections[0].tasks[0].due, Some(date(2025, 6, 15)));
    }

    #[test]
    fn empty_task_buffer_does_not_add() {
        let (file, mut app, mut list) = setup("# main\n- [ ] Existing\n");
        app.mode = Mode::InputTask(task_input(vec![0], None, None, "   "));
        handle_input_task(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();
        assert_eq!(list.sections[0].tasks.len(), 1);
    }

    #[test]
    fn due_enter_sets_clears_and_invalid_clears() {
        let (file, mut app, mut list) = setup("# main\n- [ ] Task due:2025-01-01\n");
        let target = TaskRef {
            node: vec![0],
            task_idx: 0,
            sub_name: None,
        };

        app.mode = Mode::InputDue(DueInput {
            target: target.clone(),
            field: TextField::new("2025-06-15"),
        });
        handle_input_due(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();
        assert_eq!(list.sections[0].tasks[0].due, Some(date(2025, 6, 15)));

        app.mode = Mode::InputDue(DueInput {
            target: target.clone(),
            field: TextField::new(""),
        });
        handle_input_due(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();
        assert_eq!(list.sections[0].tasks[0].due, None);

        app.mode = Mode::InputDue(DueInput {
            target,
            field: TextField::new("not-a-date"),
        });
        handle_input_due(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();
        assert_eq!(list.sections[0].tasks[0].due, None);
    }

    #[test]
    fn section_enter_creates_child_at_requested_level() {
        let (file, mut app, mut list) = setup("# main\n");
        app.left_state.select(Some(0));
        app.mode = Mode::InputSection(SectionInput {
            node: None,
            parent: Some(vec![0]),
            insert_idx: Some(0),
            level: 2,
            field: TextField::new("child"),
        });
        handle_input_section(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();

        let parsed = parse_str(&std::fs::read_to_string(file.path()).unwrap());
        assert_eq!(parsed.sections[0].children[0].name, "child");
        assert_eq!(parsed.sections[0].children[0].level, 2);
        assert_eq!(selected_node(&app), Some(vec![0, 0]));
    }

    #[test]
    fn section_enter_renames_existing() {
        let (file, mut app, mut list) = setup("# old\n");
        app.left_state.select(Some(0));
        app.mode = Mode::InputSection(SectionInput {
            node: Some(vec![0]),
            parent: None,
            insert_idx: None,
            level: 1,
            field: TextField::new("new"),
        });
        handle_input_section(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();

        let parsed = parse_str(&std::fs::read_to_string(file.path()).unwrap());
        assert_eq!(parsed.sections[0].name, "new");
        assert_eq!(selected_node(&app), Some(vec![0]));
    }

    #[test]
    fn empty_section_name_does_not_create_or_rename() {
        let (file, mut app, mut list) = setup("# main\n");
        app.mode = Mode::InputSection(SectionInput {
            node: Some(vec![0]),
            parent: None,
            insert_idx: None,
            level: 1,
            field: TextField::new("   "),
        });
        handle_input_section(
            &mut app,
            &mut list,
            file.path(),
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();
        assert_eq!(list.sections[0].name, "main");
        assert_eq!(list.sections.len(), 1);
    }
}
