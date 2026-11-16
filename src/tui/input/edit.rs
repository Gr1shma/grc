//! Input-mode key handling.
//!
//! The three input modes share the same text editor. Enter returns an action;
//! Esc leaves the document untouched.

use crate::due::{extract_inline_due_from, resolve_relative_date_from};
use crate::task::TodoList;
use crate::tui::actions::Action;
use crate::tui::input::text;
use crate::tui::state::{AppState, Mode};
use crossterm::event::{KeyCode, KeyModifiers};

/// Handle a key while creating or renaming a task.
pub fn handle_input_task(
    app: &mut AppState,
    todo_list: &TodoList,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Vec<Action> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            return Vec::new();
        }
        KeyCode::Enter => {
            let input = match &app.mode {
                Mode::InputTask(input) => input.clone(),
                _ => return Vec::new(),
            };
            let raw = input.field.buf.trim();
            app.mode = Mode::Normal;
            if raw.is_empty() {
                app.set_error("task text cannot be empty");
                return Vec::new();
            }
            let (text, due) = extract_inline_due_from(raw, app.today);
            return vec![Action::CommitTask { input, text, due }];
        }
        _ => {}
    }

    if let Mode::InputTask(input) = &mut app.mode {
        text::handle_key(&mut input.field, code, modifiers);
    }
    let _ = todo_list;
    Vec::new()
}

/// Handle a key while editing a due date.
pub fn handle_input_due(
    app: &mut AppState,
    todo_list: &TodoList,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Vec<Action> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            return Vec::new();
        }
        KeyCode::Enter => {
            let input = match &app.mode {
                Mode::InputDue(input) => input.clone(),
                _ => return Vec::new(),
            };
            let trimmed = input.field.buf.trim();
            if trimmed.is_empty() {
                app.mode = Mode::Normal;
                return vec![Action::SetDue {
                    target: input.target,
                    due: None,
                }];
            }
            let Some(due) = resolve_relative_date_from(trimmed, app.today) else {
                app.set_error("invalid due date — existing date kept");
                return Vec::new();
            };
            app.mode = Mode::Normal;
            return vec![Action::SetDue {
                target: input.target,
                due: Some(due),
            }];
        }
        _ => {}
    }

    if let Mode::InputDue(input) = &mut app.mode {
        text::handle_key(&mut input.field, code, modifiers);
    }
    let _ = todo_list;
    Vec::new()
}

/// Handle a key while creating or renaming a section.
pub fn handle_input_section(
    app: &mut AppState,
    todo_list: &TodoList,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Vec<Action> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            return Vec::new();
        }
        KeyCode::Enter => {
            let input = match &app.mode {
                Mode::InputSection(input) => input.clone(),
                _ => return Vec::new(),
            };
            let name = input.field.buf.trim().to_string();
            app.mode = Mode::Normal;
            if name.is_empty() {
                app.set_error("section name cannot be empty");
                return Vec::new();
            }
            return vec![Action::CommitSection { input, name }];
        }
        _ => {}
    }

    if let Mode::InputSection(input) = &mut app.mode {
        text::handle_key(&mut input.field, code, modifiers);
    }
    let _ = todo_list;
    Vec::new()
}

/// Handle help, filter, or conflict keys outside the normal handlers.
pub fn handle_conflict(code: KeyCode) -> Vec<Action> {
    match code {
        KeyCode::Char('r') | KeyCode::Char('R') => vec![Action::Reload],
        KeyCode::Char('o') | KeyCode::Char('O') => vec![Action::Overwrite],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::engine::Engine;
    use crate::tui::state::{MessageKind, TextField};
    use chrono::NaiveDate;
    use std::fs;
    use tempfile::TempDir;

    fn engine(content: &str) -> (TempDir, Engine) {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("todo.md");
        fs::write(&path, content).unwrap();
        (directory, Engine::load(&path).unwrap())
    }

    fn press(engine: &mut Engine, code: KeyCode) {
        engine.handle_key(code, KeyModifiers::NONE);
    }

    #[test]
    fn task_input_adds_inline_due_and_saves() {
        let (_dir, mut engine) = engine("# main\n- [ ] Existing\n");
        engine.app.focus = crate::tui::state::Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('o'));
        if let Mode::InputTask(input) = &mut engine.app.mode {
            input.field = TextField::new("New due:2025-06-15");
        }
        press(&mut engine, KeyCode::Enter);
        assert_eq!(engine.todo_list.sections[0].tasks[1].text, "New");
        assert_eq!(
            engine.todo_list.sections[0].tasks[1].due,
            NaiveDate::from_ymd_opt(2025, 6, 15)
        );
        assert_eq!(engine.app.mode, Mode::Normal);
    }

    #[test]
    fn task_input_esc_cancels_without_saving() {
        let (_dir, mut engine) = engine("# main\n- [ ] Existing\n");
        engine.app.focus = crate::tui::state::Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('i'));
        if let Mode::InputTask(input) = &mut engine.app.mode {
            input.field = TextField::new("changed");
        }
        press(&mut engine, KeyCode::Esc);
        assert_eq!(engine.todo_list.sections[0].tasks[0].text, "Existing");
        assert_eq!(engine.app.mode, Mode::Normal);
    }

    #[test]
    fn empty_task_text_reports_an_error_and_does_not_add() {
        let (_dir, mut engine) = engine("# main\n");
        press(&mut engine, KeyCode::Char('o'));
        if let Mode::InputTask(input) = &mut engine.app.mode {
            input.field = TextField::new("   ");
        }
        press(&mut engine, KeyCode::Enter);
        assert!(engine.todo_list.sections[0].tasks.is_empty());
        assert_eq!(
            engine.app.message.as_ref().unwrap().kind,
            MessageKind::Error
        );
    }

    #[test]
    fn due_input_sets_valid_date() {
        let (_dir, mut engine) = engine("# main\n- [ ] Task due:2025-01-01\n");
        engine.app.focus = crate::tui::state::Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('t'));
        if let Mode::InputDue(input) = &mut engine.app.mode {
            input.field = TextField::new("2025-06-15");
        }
        press(&mut engine, KeyCode::Enter);
        assert_eq!(
            engine.todo_list.sections[0].tasks[0].due,
            NaiveDate::from_ymd_opt(2025, 6, 15)
        );
    }

    #[test]
    fn section_input_creates_and_renames() {
        let (_dir, mut engine) = engine("# main\n");
        press(&mut engine, KeyCode::Char('o'));
        if let Mode::InputSection(input) = &mut engine.app.mode {
            input.field = TextField::new("next");
        }
        press(&mut engine, KeyCode::Enter);
        assert_eq!(engine.todo_list.sections[0].name, "main");
        assert_eq!(engine.todo_list.sections[1].name, "next");
    }
}
