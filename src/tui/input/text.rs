//! Shared single-line text editing.
//!
//! All three text modes (task, due date, section name) use the same buffer and
//! cursor rules. The cursor is a character index so insertion and deletion are
//! safe for multi-byte UTF-8 text.

use crate::tui::state::TextField;
use crossterm::event::{KeyCode, KeyModifiers};

/// Handle a key that edits `field`.
///
/// Returns `true` when the key was consumed. `false` lets the caller handle
/// Enter/Esc or any other mode-specific key.
pub fn handle_key(field: &mut TextField, code: KeyCode, modifiers: KeyModifiers) -> bool {
    match code {
        KeyCode::Backspace => {
            backspace(field);
        }
        KeyCode::Delete => {
            delete(field);
        }
        KeyCode::Left => {
            field.cursor = field.cursor.saturating_sub(1);
        }
        KeyCode::Right => {
            field.cursor = (field.cursor + 1).min(field.buf.chars().count());
        }
        KeyCode::Home => {
            field.cursor = 0;
        }
        KeyCode::End => {
            field.move_to_end();
        }
        KeyCode::Char('a') if modifiers.contains(KeyModifiers::CONTROL) => {
            field.cursor = 0;
        }
        KeyCode::Char('e') if modifiers.contains(KeyModifiers::CONTROL) => {
            field.move_to_end();
        }
        KeyCode::Char(character) => {
            insert(field, character);
        }
        _ => return false,
    }
    true
}

fn insert(field: &mut TextField, character: char) {
    let byte = byte_index(&field.buf, field.cursor);
    field.buf.insert(byte, character);
    field.cursor += 1;
}

fn backspace(field: &mut TextField) {
    if field.cursor == 0 {
        return;
    }
    let start = byte_index(&field.buf, field.cursor - 1);
    let end = byte_index(&field.buf, field.cursor);
    field.buf.replace_range(start..end, "");
    field.cursor -= 1;
}

fn delete(field: &mut TextField) {
    if field.cursor >= field.buf.chars().count() {
        return;
    }
    let start = byte_index(&field.buf, field.cursor);
    let end = byte_index(&field.buf, field.cursor + 1);
    field.buf.replace_range(start..end, "");
}

/// Convert a character index into a byte index at a character boundary.
pub fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// Split text at a character index, suitable for drawing a cursor.
pub fn split_at_char(text: &str, char_index: usize) -> (&str, &str) {
    let byte = byte_index(text, char_index);
    (&text[..byte], &text[byte..])
}
