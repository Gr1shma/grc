//! Application engine: owns the document, undo history, and persistence.
//!
//! Input handlers return [`Action`] values; this module is the only place that
//! applies document mutations and saves them. Rendering receives immutable
//! references to the engine's document and UI state.

use crate::parser::{FileStamp, parse_file, write_file};
use crate::task::TodoList;
use crate::tui::actions::{self, Action};
use crate::tui::input;
use crate::tui::state::{AppState, Mode, TreeItem};
use crate::tui::task_ref::{TaskRef, get_task_refs_filtered};
use crate::tui::tree::{
    build_visible_tree_items, clamp_left_selection, clamp_right_selection, selected_node,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Maximum number of whole-document undo snapshots.
pub const UNDO_LIMIT: usize = 100;

/// Whether the event loop should continue or exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Keep running.
    Continue,
    /// Leave the TUI.
    Quit,
}

/// Document, UI state, file stamp, and undo history.
pub struct Engine {
    /// Path shown in the status bar and used for saves.
    pub path: PathBuf,
    /// In-memory document.
    pub todo_list: TodoList,
    /// Ephemeral UI state.
    pub app: AppState,
    /// Last stamp observed after a load or save.
    pub stamp: Option<FileStamp>,
    undo: VecDeque<TodoList>,
    redo: Vec<TodoList>,
}

impl Engine {
    /// Load a document and initialize UI state.
    pub fn load(path: &Path) -> Result<Self> {
        let todo_list = parse_file(path)?;
        let stamp = FileStamp::read(path)?;
        let app = AppState::new(todo_list.sections.len());
        let mut engine = Self {
            path: path.to_path_buf(),
            todo_list,
            app,
            stamp,
            undo: VecDeque::new(),
            redo: Vec::new(),
        };
        engine.refresh();
        Ok(engine)
    }

    /// Dispatch one key and apply the resulting actions.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Control {
        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            return self.finish_quit();
        }

        self.app.clear_message();
        let actions = input::handle_key(&mut self.app, &self.todo_list, code, modifiers);
        self.run_actions(actions)
    }

    fn run_actions(&mut self, actions: Vec<Action>) -> Control {
        let mut changed = false;
        let mut snapshot_taken = false;

        for action in actions {
            match action {
                Action::Quit => return self.finish_quit(),
                Action::Reload => {
                    self.reload();
                    return Control::Continue;
                }
                Action::Overwrite => {
                    self.overwrite();
                    return Control::Continue;
                }
                Action::Undo => {
                    self.undo();
                    return Control::Continue;
                }
                Action::Redo => {
                    self.redo();
                    return Control::Continue;
                }
                action => {
                    if !snapshot_taken {
                        self.push_undo();
                        snapshot_taken = true;
                    }
                    if actions::apply(action, &mut self.app, &mut self.todo_list) {
                        changed = true;
                    }
                }
            }
        }

        if snapshot_taken && !changed {
            self.undo.pop_back();
        }
        if changed {
            self.redo.clear();
            self.persist();
        }
        self.refresh();
        Control::Continue
    }

    /// Apply actions directly. Useful for integration tests.
    #[cfg(test)]
    pub fn apply_actions(&mut self, actions: Vec<Action>) -> Control {
        let control = self.run_actions(actions);
        self.refresh();
        control
    }

    fn push_undo(&mut self) {
        if self.undo.len() == UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(self.todo_list.clone());
    }

    fn undo(&mut self) {
        let Some(previous) = self.undo.pop_back() else {
            self.app.set_info("nothing to undo");
            return;
        };
        let current = std::mem::replace(&mut self.todo_list, previous);
        self.redo.push(current);
        self.app.set_success("undone");
        self.persist();
        self.refresh();
    }

    fn redo(&mut self) {
        let Some(next) = self.redo.pop() else {
            self.app.set_info("nothing to redo");
            return;
        };
        let current = std::mem::replace(&mut self.todo_list, next);
        if self.undo.len() == UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(current);
        self.app.set_success("redone");
        self.persist();
        self.refresh();
    }

    fn finish_quit(&mut self) -> Control {
        if self.app.mode == Mode::Conflict {
            self.app
                .set_error("resolve the file conflict before quitting (r/o)");
            return Control::Continue;
        }
        self.persist();
        if matches!(
            self.app.message.as_ref().map(|m| m.kind),
            Some(crate::tui::state::MessageKind::Error)
        ) {
            Control::Continue
        } else {
            Control::Quit
        }
    }

    fn persist(&mut self) {
        if self.app.mode == Mode::Conflict {
            return;
        }
        match FileStamp::read(&self.path) {
            Ok(current) if current != self.stamp => {
                self.enter_conflict();
                return;
            }
            Ok(_) => {}
            Err(error) => {
                self.app.set_error(format!("save failed: {error:#}"));
                return;
            }
        }

        match write_file(&self.path, &self.todo_list) {
            Ok(()) => {
                self.stamp = FileStamp::read(&self.path).ok().flatten();
                if self.app.message.is_none() {
                    self.app.set_success("saved");
                }
            }
            Err(error) => self.app.set_error(format!("save failed: {error:#}")),
        }
    }

    fn enter_conflict(&mut self) {
        self.app.mode = Mode::Conflict;
        self.app
            .set_error("file changed on disk — r reload / o overwrite");
    }

    fn reload(&mut self) {
        match parse_file(&self.path) {
            Ok(todo_list) => {
                let today = self.app.today;
                self.todo_list = todo_list;
                self.stamp = FileStamp::read(&self.path).ok().flatten();
                self.app = AppState::new_with_today(self.todo_list.sections.len(), today);
                self.undo.clear();
                self.redo.clear();
                self.app.set_success("reloaded from disk");
                self.refresh();
            }
            Err(error) => self.app.set_error(format!("reload failed: {error:#}")),
        }
    }

    fn overwrite(&mut self) {
        match write_file(&self.path, &self.todo_list) {
            Ok(()) => {
                self.stamp = FileStamp::read(&self.path).ok().flatten();
                self.app.mode = Mode::Normal;
                self.app.set_success("overwrote disk changes");
                self.refresh();
            }
            Err(error) => self.app.set_error(format!("save failed: {error:#}")),
        }
    }

    /// Rebuild derived view state without mutating the document.
    pub fn refresh(&mut self) {
        self.app.tree_items = build_visible_tree_items(
            &self.todo_list,
            &self.app.mode,
            &self.app.filter_lower,
            &self.app.folded,
            self.app.hide_done,
        );

        self.restore_filter_selection();

        let mode = self.app.mode.clone();
        if matches!(&mode, Mode::Filter)
            && let Some(anchor) = self.app.filter_anchor.clone()
            && self.select_path(&anchor)
        {
            self.clamp_right_selection_for_node();
            return;
        }

        match mode {
            Mode::InputSection(input) if input.node.is_none() => {
                self.select_ghost();
            }
            Mode::InputSection(input) => {
                if let Some(node) = &input.node {
                    self.select_path(node);
                }
            }
            Mode::InputTask(input) => {
                self.select_path(&input.node);
                let refs = self.refs_for(&input.node);
                let row = match input.editing_idx {
                    Some(index) => index,
                    None => input.insert_idx.unwrap_or(refs.len()),
                };
                if input.editing_idx.is_some() {
                    if refs.is_empty() {
                        self.app.right_state.select(None);
                    } else {
                        self.app.right_state.select(Some(row.min(refs.len() - 1)));
                    }
                } else {
                    self.app.right_state.select(Some(row.min(refs.len())));
                }
            }
            _ => {
                self.app.filter_anchor = None;
                clamp_left_selection(&mut self.app);
                self.clamp_right_selection_for_node();
            }
        }
    }

    fn refs_for(&self, node: &[usize]) -> Vec<TaskRef> {
        get_task_refs_filtered(
            &self.todo_list,
            &node.to_vec(),
            &self.app.filter_lower,
            self.app.hide_done,
        )
    }

    fn restore_filter_selection(&mut self) {
        let Some(snapshot) = self.app.filter_restore.take() else {
            return;
        };

        if let Some(node) = &snapshot.node {
            self.select_path(node);
        }

        if let Some(task) = snapshot.task {
            let refs = self.refs_for(&task.node);
            if let Some(position) = find_task_position(&refs, &task) {
                self.app.right_state.select(Some(position));
            } else {
                self.app.right_state.select(None);
            }
        }
    }

    fn clamp_right_selection_for_node(&mut self) {
        let row_count = selected_node(&self.app).map_or(0, |node| self.refs_for(&node).len());
        clamp_right_selection(&mut self.app, row_count);
    }

    fn select_path(&mut self, path: &[usize]) -> bool {
        if let Some(position) = self
            .app
            .tree_items
            .iter()
            .position(|item| matches!(item, TreeItem::Node(p) if p.as_slice() == path))
        {
            self.app.left_state.select(Some(position));
            true
        } else {
            false
        }
    }

    fn select_ghost(&mut self) {
        if let Some(position) = self
            .app
            .tree_items
            .iter()
            .position(|item| matches!(item, TreeItem::Ghost(_)))
        {
            self.app.left_state.select(Some(position));
        } else {
            clamp_left_selection(&mut self.app);
        }
    }
}

fn find_task_position(refs: &[TaskRef], task: &TaskRef) -> Option<usize> {
    refs.iter()
        .position(|reference| reference.node == task.node && reference.task_idx == task.task_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::to_markdown;
    use crate::task::Task;
    use crate::tui::state::{Focus, MessageKind, TextField};
    use crate::tui::task_ref::TaskRef;
    use chrono::NaiveDate;
    use std::fs;
    use tempfile::TempDir;

    fn engine(content: &str) -> (TempDir, Engine) {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("todo.md");
        fs::write(&path, content).unwrap();
        (directory, Engine::load(&path).unwrap())
    }

    fn press(engine: &mut Engine, code: KeyCode) -> Control {
        engine.handle_key(code, KeyModifiers::NONE)
    }

    #[test]
    fn load_selects_first_section_and_stamps_the_file() {
        let (_dir, engine) = engine("# main\n- [ ] A\n");
        assert_eq!(engine.app.left_state.selected(), Some(0));
        assert!(engine.stamp.is_some());
        assert_eq!(engine.todo_list.sections[0].tasks.len(), 1);
    }

    #[test]
    fn every_mutation_saves_and_quit_saves_again() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n");
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));
        assert_eq!(press(&mut engine, KeyCode::Char('x')), Control::Continue);
        assert!(
            fs::read_to_string(&engine.path)
                .unwrap()
                .contains("- [x] A")
        );
        assert_eq!(press(&mut engine, KeyCode::Char('q')), Control::Quit);
    }

    #[test]
    fn invalid_due_input_keeps_old_date_and_stays_in_input_mode() {
        let (_dir, mut engine) = engine("# main\n- [ ] A due:2025-06-15\n");
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('t'));
        match &mut engine.app.mode {
            Mode::InputDue(input) => input.field = TextField::new("nonsense"),
            _ => panic!("expected due input"),
        }
        press(&mut engine, KeyCode::Enter);
        assert_eq!(
            engine.todo_list.sections[0].tasks[0].due,
            NaiveDate::from_ymd_opt(2025, 6, 15)
        );
        assert!(matches!(engine.app.mode, Mode::InputDue(_)));
        assert_eq!(
            engine.app.message.as_ref().unwrap().kind,
            MessageKind::Error
        );
        assert!(
            fs::read_to_string(&engine.path)
                .unwrap()
                .contains("2025-06-15")
        );
    }

    #[test]
    fn empty_due_input_clears_the_date() {
        let (_dir, mut engine) = engine("# main\n- [ ] A due:2025-06-15\n");
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('t'));
        if let Mode::InputDue(input) = &mut engine.app.mode {
            input.field = TextField::new("");
        }
        press(&mut engine, KeyCode::Enter);
        assert!(engine.todo_list.sections[0].tasks[0].due.is_none());
        assert!(!fs::read_to_string(&engine.path).unwrap().contains("due:"));
    }

    #[test]
    fn external_change_blocks_save_and_can_reload_or_overwrite() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n");
        fs::write(&engine.path, "# main\n- [ ] External\n").unwrap();

        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('x'));
        assert_eq!(engine.app.mode, Mode::Conflict);
        assert!(
            fs::read_to_string(&engine.path)
                .unwrap()
                .contains("External")
        );
        assert!(
            engine
                .app
                .message
                .as_ref()
                .is_some_and(|message| message.kind == MessageKind::Error)
        );

        press(&mut engine, KeyCode::Char('r'));
        assert_eq!(engine.app.mode, Mode::Normal);
        assert_eq!(engine.todo_list.sections[0].tasks[0].text, "External");
        assert!(!engine.todo_list.sections[0].tasks[0].is_done);

        fs::write(&engine.path, "# main\n- [ ] Again\n").unwrap();
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('x'));
        press(&mut engine, KeyCode::Char('o'));
        assert_eq!(engine.app.mode, Mode::Normal);
        assert!(
            fs::read_to_string(&engine.path)
                .unwrap()
                .contains("- [x] External")
        );
    }

    #[test]
    fn save_errors_are_reported_but_do_not_exit() {
        let (directory, mut engine) = engine("# main\n- [ ] A\n");
        engine.stamp = None;
        fs::remove_dir_all(directory.path()).unwrap();
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));
        let control = press(&mut engine, KeyCode::Char('x'));
        assert_eq!(control, Control::Continue);
        assert!(
            engine
                .app
                .message
                .as_ref()
                .is_some_and(|message| message.kind == MessageKind::Error)
        );
        assert!(engine.todo_list.sections[0].tasks[0].is_done);
    }

    #[test]
    fn undo_redo_restore_whole_documents_and_are_bounded() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n");
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('x'));
        press(&mut engine, KeyCode::Char('u'));
        assert!(!engine.todo_list.sections[0].tasks[0].is_done);
        engine.handle_key(KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert!(engine.todo_list.sections[0].tasks[0].is_done);

        for index in 0..(UNDO_LIMIT + 5) {
            engine.todo_list.sections[0]
                .tasks
                .push(Task::new(format!("task {index}")));
            engine.push_undo();
        }
        assert_eq!(engine.undo.len(), UNDO_LIMIT);
    }

    #[test]
    fn quit_with_conflict_does_not_leave_the_app() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n");
        fs::write(&engine.path, "# main\n- [ ] External\n").unwrap();
        assert_eq!(press(&mut engine, KeyCode::Char('q')), Control::Continue);
        assert_eq!(engine.app.mode, Mode::Conflict);
    }

    #[test]
    fn apply_actions_uses_the_same_central_path() {
        let (_dir, mut engine) = engine("# main\n");
        let target = TaskRef {
            node: vec![0],
            task_idx: 0,
            sub_name: None,
        };
        engine.todo_list.sections[0].tasks.push(Task::new("A"));
        assert_eq!(
            engine.apply_actions(vec![Action::ToggleTask {
                target: target.clone()
            }]),
            Control::Continue
        );
        assert!(engine.todo_list.sections[0].tasks[0].is_done);
        assert!(to_markdown(&engine.todo_list).contains("- [x] A"));
    }
}
