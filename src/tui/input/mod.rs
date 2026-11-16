//! Key dispatch for every input mode.
//!
//! Handlers never write files. They mutate ephemeral UI state and return
//! [`Action`] values for the engine to apply in one place.

mod edit;
mod normal;
pub(crate) mod text;

use crate::task::TodoList;
use crate::tui::actions::Action;
use crate::tui::state::{AppState, Mode};
use crossterm::event::{KeyCode, KeyModifiers};

/// Dispatch one key event.
pub fn handle_key(
    app: &mut AppState,
    todo_list: &TodoList,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Vec<Action> {
    match app.mode.clone() {
        Mode::Normal => normal::handle_normal(app, todo_list, code, modifiers),
        Mode::Help => normal::handle_help(app, code),
        Mode::Filter => normal::handle_filter(app, todo_list, code),
        Mode::InputTask(_) => edit::handle_input_task(app, todo_list, code, modifiers),
        Mode::InputDue(_) => edit::handle_input_due(app, todo_list, code, modifiers),
        Mode::InputSection(_) => edit::handle_input_section(app, todo_list, code, modifiers),
        Mode::Conflict => edit::handle_conflict(code),
    }
}
