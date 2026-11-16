//! Key dispatch for every input mode.
//!
//! This is the single place that maps the current mode to a handler. Mode
//! handlers own their text buffers and either mutate the document immediately
//! (normal mode) or wait for Enter (input modes).

mod edit;
mod normal;
pub(crate) mod text;

use crate::task::TodoList;
use crate::tui::state::{AppState, Mode};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::Path;

/// Dispatch one key event.
///
/// Returns `true` when the application should quit.
pub fn handle_key(
    app: &mut AppState,
    todo_list: &mut TodoList,
    path: &Path,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Result<bool> {
    match app.mode.clone() {
        Mode::Normal => normal::handle_normal(app, todo_list, path, code),
        Mode::Help => {
            handle_help(app, code);
            Ok(false)
        }
        Mode::Filter => {
            normal::handle_filter(app, todo_list, code)?;
            Ok(false)
        }
        Mode::InputTask(_) => {
            edit::handle_input_task(app, todo_list, path, code, modifiers)?;
            Ok(false)
        }
        Mode::InputDue(_) => {
            edit::handle_input_due(app, todo_list, path, code, modifiers)?;
            Ok(false)
        }
        Mode::InputSection(_) => {
            edit::handle_input_section(app, todo_list, path, code, modifiers)?;
            Ok(false)
        }
    }
}

fn handle_help(app: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.mode = Mode::Normal;
        }
        KeyCode::Char('/') => {
            app.filter_anchor = None;
            app.left_state.select(Some(0));
            app.right_state.select(Some(0));
            app.mode = Mode::Filter;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.help_scroll = app.help_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }
        _ => {}
    }
}
