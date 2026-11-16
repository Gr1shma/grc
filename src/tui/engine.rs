//! Application engine: owns the document, undo history, and persistence.
//!
//! Input handlers return [`Action`] values; this module is the only place that
//! applies document mutations and saves them. Rendering receives immutable
//! references to the engine's document and UI state.

use crate::parser::{FileStamp, parse_file, write_file};
use crate::task::{TodoList, get_node};
use crate::tui::actions::{self, Action};
use crate::tui::input;
use crate::tui::state::{AppState, Mode, TreeItem};
use crate::tui::task_ref::{TaskRef, get_task_refs_filtered};
use crate::tui::theme::Theme;
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
    /// Palette and glyphs, resolved once from the environment.
    pub theme: Theme,
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
            theme: Theme::from_env(),
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
            Err(error) => {
                self.stamp = FileStamp::read(&self.path).ok().flatten();
                self.app.set_error(format!("save failed: {error:#}"));
            }
        }
    }

    fn enter_conflict(&mut self) {
        self.app.mode = Mode::Conflict;
        self.app.set_error("file changed on disk — choose below");
    }

    /// Replace the document with the one on disk.
    ///
    /// Only the undo history is dropped: the view the user was looking at
    /// (focus, folds, filter, selection) survives wherever the section or task
    /// still exists, and the status bar says so when it does not.
    fn reload(&mut self) {
        let todo_list = match parse_file(&self.path) {
            Ok(todo_list) => todo_list,
            Err(error) => {
                self.app.set_error(format!("reload failed: {error:#}"));
                return;
            }
        };

        let node = selected_node(&self.app);
        let task = node.as_ref().and_then(|node| {
            let refs = self.refs_for(node);
            self.app
                .right_state
                .selected()
                .and_then(|row| refs.get(row).cloned())
        });
        let folded = self.app.folded.len();

        self.todo_list = todo_list;
        self.stamp = FileStamp::read(&self.path).ok().flatten();
        self.undo.clear();
        self.redo.clear();
        self.app.mode = Mode::Normal;
        self.app.clear_pending();
        self.app.help_scroll = 0;
        self.app.clipboard = None;
        self.app.filter_anchor = None;
        self.app.filter_restore = None;
        // A fold for a section that is gone would fold nothing.
        self.app
            .folded
            .retain(|path| get_node(&self.todo_list, path).is_some());

        self.refresh();

        let kept_node = node.is_some_and(|node| self.select_path(&node));
        let kept_task = task.is_some_and(|task| {
            let refs = self.refs_for(&task.node);
            match find_task_position(&refs, &task) {
                Some(position) => {
                    self.app.right_state.select(Some(position));
                    true
                }
                None => false,
            }
        });

        let mut message = "reloaded from disk".to_string();
        let mut reset = Vec::new();
        if !kept_node {
            reset.push("selection");
        } else if !kept_task {
            reset.push("task row");
        }
        if folded != self.app.folded.len() {
            reset.push("folds of sections that are gone");
        }
        if !reset.is_empty() {
            message.push_str(&format!(" — reset: {}", reset.join(", ")));
        }
        self.app.set_success(message);
    }

    fn overwrite(&mut self) {
        match write_file(&self.path, &self.todo_list) {
            Ok(()) => {
                self.stamp = FileStamp::read(&self.path).ok().flatten();
                self.app.mode = Mode::Normal;
                self.app.set_success("overwrote disk changes");
                self.refresh();
            }
            Err(error) => {
                self.stamp = FileStamp::read(&self.path).ok().flatten();
                self.app.set_error(format!("save failed: {error:#}"));
            }
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
    use crate::tui::state::{ClipboardItem, Focus, MessageKind, TextField};
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
    fn reload_keeps_the_view_and_only_drops_the_undo_history() {
        let (_dir, mut engine) = engine(
            "# Work\n- [ ] Ship\n- [ ] Docs\n## Urgent\n- [ ] Fix bug\n# Home\n- [ ] Milk\n",
        );
        engine.app.focus = Focus::Right;
        engine.app.folded.insert(vec![1]);
        engine.app.right_state.select(Some(1));
        press(&mut engine, KeyCode::Char('x'));
        assert!(!engine.undo.is_empty());

        engine.reload();

        assert_eq!(engine.app.mode, Mode::Normal);
        assert_eq!(engine.app.focus, Focus::Right);
        assert!(engine.app.folded.contains(&vec![1]));
        assert_eq!(selected_node(&engine.app), Some(vec![0]));
        assert_eq!(engine.app.right_state.selected(), Some(1));
        assert!(engine.undo.is_empty() && engine.redo.is_empty());
        assert!(
            engine
                .app
                .message
                .as_ref()
                .unwrap()
                .text
                .starts_with("reloaded")
        );
    }

    #[test]
    fn reload_keeps_the_filter() {
        let (_dir, mut engine) = engine("# Work\n- [ ] Ship\n# Home\n- [ ] Milk\n");
        engine.app.set_filter("milk".into());
        engine.refresh();
        assert_eq!(engine.app.tree_items.len(), 1);

        engine.reload();

        assert_eq!(engine.app.filter, "milk");
        assert_eq!(engine.app.filter_lower, "milk");
        assert_eq!(engine.app.tree_items.len(), 1);
    }

    #[test]
    fn reload_reports_a_selection_that_no_longer_exists() {
        let (_dir, mut engine) = engine("# Work\n- [ ] Ship\n# Home\n- [ ] Milk\n");
        engine.app.left_state.select(Some(1));
        fs::write(&engine.path, "# Work\n- [ ] Ship\n").unwrap();

        engine.reload();

        assert_eq!(engine.todo_list.sections.len(), 1);
        assert_eq!(engine.app.left_state.selected(), Some(0));
        assert!(
            engine
                .app
                .message
                .as_ref()
                .unwrap()
                .text
                .contains("reset: selection")
        );
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

    #[test]
    fn count_prefixes_drive_motions_and_multi_delete() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n- [ ] B\n- [ ] C\n- [ ] D\n- [ ] E\n");
        engine.app.focus = Focus::Right;
        press(&mut engine, KeyCode::Char('5'));
        press(&mut engine, KeyCode::Char('j'));
        assert_eq!(engine.app.right_state.selected(), Some(4));

        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('3'));
        press(&mut engine, KeyCode::Char('d'));
        press(&mut engine, KeyCode::Char('d'));
        let texts: Vec<_> = engine.todo_list.sections[0]
            .tasks
            .iter()
            .map(|task| task.text.as_str())
            .collect();
        assert_eq!(texts, ["D", "E"]);
        assert!(
            fs::read_to_string(&engine.path)
                .unwrap()
                .contains("- [ ] D")
        );

        press(&mut engine, KeyCode::Char('u'));
        assert_eq!(engine.todo_list.sections[0].tasks.len(), 5);
        engine.handle_key(KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(engine.todo_list.sections[0].tasks.len(), 2);
    }

    #[test]
    fn fold_keys_collapse_and_expand_the_tree() {
        let (_dir, mut engine) = engine("# root\n## child\n### grand\n# other\n");
        assert_eq!(engine.app.tree_items.len(), 4);
        press(&mut engine, KeyCode::Char('z'));
        press(&mut engine, KeyCode::Char('a'));
        assert_eq!(engine.app.tree_items.len(), 2);
        assert!(engine.app.folded.contains(&vec![0]));

        press(&mut engine, KeyCode::Char('z'));
        press(&mut engine, KeyCode::Char('R'));
        assert!(engine.app.folded.is_empty());
        assert_eq!(engine.app.tree_items.len(), 4);

        press(&mut engine, KeyCode::Char('z'));
        press(&mut engine, KeyCode::Char('M'));
        assert!(engine.app.folded.contains(&vec![0]));
        assert!(engine.app.folded.contains(&vec![0, 0]));
    }

    #[test]
    fn hide_done_toggle_only_changes_the_view() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n- [x] B\n");
        let before = fs::read_to_string(&engine.path).unwrap();
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(1));
        press(&mut engine, KeyCode::Char('H'));
        assert!(engine.app.hide_done);
        assert_eq!(engine.app.right_state.selected(), Some(0));
        assert_eq!(
            get_task_refs_filtered(
                &engine.todo_list,
                &vec![0],
                &engine.app.filter_lower,
                engine.app.hide_done
            )
            .len(),
            1
        );

        press(&mut engine, KeyCode::Char('H'));
        assert!(!engine.app.hide_done);
        assert_eq!(fs::read_to_string(&engine.path).unwrap(), before);
    }

    #[test]
    fn next_overdue_wraps_and_skips_completed_tasks() {
        let today = NaiveDate::from_ymd_opt(2025, 7, 21).unwrap();
        let yesterday = today.pred_opt().unwrap();
        let two_days_ago = today.pred_opt().unwrap().pred_opt().unwrap();
        let tomorrow = today.succ_opt().unwrap();
        let content = format!(
            "# main\n- [ ] A due:{}\n- [x] B due:{}\n- [ ] C due:{}\n- [ ] D due:{}\n- [ ] E\n",
            yesterday.format("%Y-%m-%d"),
            two_days_ago.format("%Y-%m-%d"),
            two_days_ago.format("%Y-%m-%d"),
            tomorrow.format("%Y-%m-%d"),
        );
        let (_dir, mut engine) = engine(&content);
        engine.app.today = today;
        engine.refresh();
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));

        press(&mut engine, KeyCode::Char(']'));
        press(&mut engine, KeyCode::Char('d'));
        assert_eq!(engine.app.right_state.selected(), Some(2));
        assert_eq!(engine.app.focus, Focus::Right);

        press(&mut engine, KeyCode::Char(']'));
        press(&mut engine, KeyCode::Char('d'));
        assert_eq!(engine.app.right_state.selected(), Some(0));
    }

    #[test]
    fn next_overdue_searches_the_whole_document() {
        let today = NaiveDate::from_ymd_opt(2025, 7, 21).unwrap();
        let yesterday = today.pred_opt().unwrap();
        let content = format!(
            "# first\n- [ ] nothing due\n# second\n- [ ] late due:{}\n",
            yesterday.format("%Y-%m-%d"),
        );
        let (_dir, mut engine) = engine(&content);
        engine.app.today = today;
        engine.refresh();
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));

        press(&mut engine, KeyCode::Char(']'));
        press(&mut engine, KeyCode::Char('d'));
        assert_eq!(
            crate::tui::tree::selected_node(&engine.app),
            Some(vec![1]),
            "the overdue task lives in another section"
        );
        assert_eq!(engine.app.right_state.selected(), Some(0));
        assert_eq!(engine.app.focus, Focus::Right);
        assert_eq!(
            engine.app.message.as_ref().unwrap().text,
            "next overdue task"
        );
    }

    #[test]
    fn next_overdue_unfolds_the_ancestors_of_a_hidden_task() {
        let today = NaiveDate::from_ymd_opt(2025, 7, 21).unwrap();
        let yesterday = today.pred_opt().unwrap();
        let content = format!(
            "# first\n- [ ] nothing due\n## buried\n- [ ] late due:{}\n",
            yesterday.format("%Y-%m-%d"),
        );
        let (_dir, mut engine) = engine(&content);
        engine.app.today = today;
        engine.refresh();
        engine.app.folded.insert(vec![0]);
        engine.refresh();
        assert_eq!(engine.app.tree_items.len(), 1, "the subtree starts folded");

        press(&mut engine, KeyCode::Char(']'));
        press(&mut engine, KeyCode::Char('d'));
        assert!(
            engine.app.folded.is_empty(),
            "reaching the task unfolds its ancestors"
        );
        assert_eq!(
            crate::tui::tree::selected_node(&engine.app),
            Some(vec![0, 0])
        );
        assert_eq!(engine.app.right_state.selected(), Some(0));
        assert_eq!(engine.app.focus, Focus::Right);
        assert_eq!(
            engine.app.message.as_ref().unwrap().text,
            "next overdue task — unfolded to reach it"
        );
    }

    #[test]
    fn next_overdue_clears_a_filter_that_hides_the_task() {
        let today = NaiveDate::from_ymd_opt(2025, 7, 21).unwrap();
        let yesterday = today.pred_opt().unwrap();
        let content = format!(
            "# first\n- [ ] nothing due\n# second\n- [ ] late due:{}\n",
            yesterday.format("%Y-%m-%d"),
        );
        let (_dir, mut engine) = engine(&content);
        engine.app.today = today;
        engine.app.set_filter("nothing".into());
        engine.refresh();

        press(&mut engine, KeyCode::Char(']'));
        press(&mut engine, KeyCode::Char('d'));
        assert_eq!(
            crate::tui::tree::selected_node(&engine.app),
            Some(vec![1]),
            "the filter must not hide the jump target"
        );
        assert_eq!(engine.app.filter, "");
        assert_eq!(engine.app.right_state.selected(), Some(0));
        assert_eq!(
            engine.app.message.as_ref().unwrap().text,
            "next overdue task — filter cleared to reach it"
        );
    }

    #[test]
    fn next_overdue_keeps_a_filter_that_does_not_hide_the_task() {
        let today = NaiveDate::from_ymd_opt(2025, 7, 21).unwrap();
        let yesterday = today.pred_opt().unwrap();
        let content = format!(
            "# first\n- [ ] late due:{}\n# second\n- [ ] later due:{}\n",
            yesterday.format("%Y-%m-%d"),
            yesterday.format("%Y-%m-%d"),
        );
        let (_dir, mut engine) = engine(&content);
        engine.app.today = today;
        engine.app.set_filter("late".into());
        engine.refresh();

        press(&mut engine, KeyCode::Char(']'));
        press(&mut engine, KeyCode::Char('d'));
        assert_eq!(engine.app.filter, "late", "a harmless filter stays");
        assert_eq!(
            crate::tui::tree::selected_node(&engine.app),
            Some(vec![1]),
            "the jump moves on from the task it was sitting on"
        );
        assert_eq!(
            engine.app.message.as_ref().unwrap().text,
            "next overdue task"
        );
    }

    #[test]
    fn shift_j_and_k_reorder_tasks_and_sections() {
        let (_dir, mut engine) = engine("# a\n- [ ] A\n- [ ] B\n# b\n- [ ] Bb\n");
        engine.app.focus = Focus::Right;
        press(&mut engine, KeyCode::Char('J'));
        assert_eq!(engine.todo_list.sections[0].tasks[0].text, "B");
        assert_eq!(engine.app.right_state.selected(), Some(1));
        press(&mut engine, KeyCode::Char('K'));
        assert_eq!(engine.todo_list.sections[0].tasks[0].text, "A");

        engine.app.focus = Focus::Left;
        engine.app.left_state.select(Some(1));
        press(&mut engine, KeyCode::Char('K'));
        assert_eq!(engine.todo_list.sections[0].name, "b");
        assert_eq!(crate::tui::tree::selected_node(&engine.app), Some(vec![0]));
    }

    #[test]
    fn colon_sort_key_sorts_recursively_and_ignores_a_second_noop() {
        let (_dir, mut engine) = engine(
            "# main\n- [ ] late due:2025-12-01\n- [ ] soon due:2025-06-01\n- [ ] none\n## child\n- [ ] child late due:2025-11-01\n- [ ] child soon due:2025-05-01\n",
        );
        engine.app.focus = Focus::Right;
        press(&mut engine, KeyCode::Char(':'));
        let texts: Vec<_> = engine.todo_list.sections[0]
            .tasks
            .iter()
            .map(|task| task.text.as_str())
            .collect();
        assert_eq!(texts, ["soon", "late", "none"]);
        let child_texts: Vec<_> = engine.todo_list.sections[0].children[0]
            .tasks
            .iter()
            .map(|task| task.text.as_str())
            .collect();
        assert_eq!(child_texts, ["child soon", "child late"]);
        assert_eq!(engine.undo.len(), 1);

        press(&mut engine, KeyCode::Char(':'));
        assert_eq!(engine.undo.len(), 1);
        assert_eq!(engine.app.message.as_ref().unwrap().text, "already sorted");
    }

    #[test]
    fn help_overlay_opens_scrolls_and_closes_on_help_keys() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n");
        press(&mut engine, KeyCode::Char('?'));
        assert_eq!(engine.app.mode, Mode::Help);
        assert_eq!(engine.app.help_scroll, 0);
        press(&mut engine, KeyCode::Char('/'));
        assert_eq!(engine.app.mode, Mode::Help);
        assert!(engine.app.filter.is_empty());
        press(&mut engine, KeyCode::Char('j'));
        assert_eq!(engine.app.help_scroll, 1);
        press(&mut engine, KeyCode::Char('q'));
        assert_eq!(engine.app.mode, Mode::Normal);
    }

    #[test]
    fn filter_key_shows_matches_and_esc_restores_selection() {
        let (_dir, mut engine) = engine("# alpha\n- [ ] apple\n# beta\n- [ ] banana\n");
        engine.app.focus = Focus::Right;
        engine.app.right_state.select(Some(0));
        press(&mut engine, KeyCode::Char('/'));
        assert_eq!(engine.app.mode, Mode::Filter);
        for character in "app".chars() {
            press(&mut engine, KeyCode::Char(character));
        }
        press(&mut engine, KeyCode::Enter);
        assert_eq!(engine.app.mode, Mode::Normal);
        assert_eq!(engine.app.filter, "app");
        assert_eq!(engine.app.tree_items.len(), 1);
        press(&mut engine, KeyCode::Esc);
        assert!(engine.app.filter.is_empty());
        assert_eq!(engine.app.tree_items.len(), 2);
    }

    #[test]
    fn add_task_key_creates_saves_and_returns_to_normal() {
        let (_dir, mut engine) = engine("# main\n");
        engine.app.focus = Focus::Right;
        press(&mut engine, KeyCode::Char('a'));
        assert!(matches!(engine.app.mode, Mode::InputTask(_)));
        for character in "New".chars() {
            press(&mut engine, KeyCode::Char(character));
        }
        press(&mut engine, KeyCode::Enter);
        assert_eq!(engine.app.mode, Mode::Normal);
        assert_eq!(engine.todo_list.sections[0].tasks[0].text, "New");
        assert!(
            fs::read_to_string(&engine.path)
                .unwrap()
                .contains("- [ ] New")
        );
    }

    #[test]
    fn navigation_keys_switch_panels_and_move_selection() {
        let (_dir, mut engine) = engine("# a\n- [ ] A\n- [ ] B\n# b\n");
        press(&mut engine, KeyCode::Char('j'));
        assert_eq!(engine.app.left_state.selected(), Some(1));
        press(&mut engine, KeyCode::Char('k'));
        assert_eq!(engine.app.left_state.selected(), Some(0));

        press(&mut engine, KeyCode::Char('l'));
        assert_eq!(engine.app.focus, Focus::Right);
        press(&mut engine, KeyCode::Char('j'));
        assert_eq!(engine.app.right_state.selected(), Some(1));
        press(&mut engine, KeyCode::Char('k'));
        assert_eq!(engine.app.right_state.selected(), Some(0));
        press(&mut engine, KeyCode::Char('G'));
        assert_eq!(engine.app.right_state.selected(), Some(1));
        press(&mut engine, KeyCode::Char('g'));
        press(&mut engine, KeyCode::Char('g'));
        assert_eq!(engine.app.right_state.selected(), Some(0));

        press(&mut engine, KeyCode::Char('h'));
        assert_eq!(engine.app.focus, Focus::Left);
        press(&mut engine, KeyCode::Tab);
        assert_eq!(engine.app.focus, Focus::Right);
        press(&mut engine, KeyCode::Esc);
        assert_eq!(engine.app.focus, Focus::Left);
        press(&mut engine, KeyCode::Char('q'));
    }

    #[test]
    fn toggle_keys_space_x_and_enter_change_done_state() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n");
        engine.app.focus = Focus::Right;
        press(&mut engine, KeyCode::Char(' '));
        assert!(engine.todo_list.sections[0].tasks[0].is_done);
        press(&mut engine, KeyCode::Char('x'));
        assert!(!engine.todo_list.sections[0].tasks[0].is_done);
        press(&mut engine, KeyCode::Enter);
        assert!(engine.todo_list.sections[0].tasks[0].is_done);
    }

    #[test]
    fn yank_and_paste_keys_copy_tasks_and_sections() {
        let (_dir, mut engine) = engine("# a\n- [ ] A\n- [ ] B\n# b\n");
        engine.app.focus = Focus::Right;
        press(&mut engine, KeyCode::Char('y'));
        press(&mut engine, KeyCode::Char('y'));
        assert!(matches!(engine.app.clipboard, Some(ClipboardItem::Task(_))));
        press(&mut engine, KeyCode::Char('p'));
        assert_eq!(engine.todo_list.sections[0].tasks.len(), 3);
        press(&mut engine, KeyCode::Char('P'));
        assert_eq!(engine.todo_list.sections[0].tasks.len(), 4);

        engine.app.focus = Focus::Left;
        engine.app.left_state.select(Some(0));
        press(&mut engine, KeyCode::Char('y'));
        press(&mut engine, KeyCode::Char('y'));
        assert!(matches!(
            engine.app.clipboard,
            Some(ClipboardItem::Section(_))
        ));
        press(&mut engine, KeyCode::Char('p'));
        assert_eq!(engine.todo_list.sections.len(), 3);
    }

    #[test]
    fn section_keys_target_the_right_section() {
        let (_dir, mut engine) = engine("# root\n## child\n# other\n");
        engine.app.left_state.select(Some(0));
        press(&mut engine, KeyCode::Char('a'));
        match &engine.app.mode {
            Mode::InputSection(input) => {
                assert_eq!(input.parent, Some(vec![0]));
                assert_eq!(input.insert_idx, Some(0));
                assert_eq!(input.level, 2);
            }
            _ => panic!("expected section input"),
        }
        press(&mut engine, KeyCode::Esc);

        engine.app.left_state.select(Some(0));
        press(&mut engine, KeyCode::Char('o'));
        match &engine.app.mode {
            Mode::InputSection(input) => {
                assert_eq!(input.parent, None);
                assert_eq!(input.insert_idx, Some(1));
                assert_eq!(input.level, 1);
            }
            _ => panic!("expected section input"),
        }
        press(&mut engine, KeyCode::Esc);

        engine.app.left_state.select(Some(0));
        press(&mut engine, KeyCode::Char('O'));
        match &engine.app.mode {
            Mode::InputSection(input) => assert_eq!(input.insert_idx, Some(0)),
            _ => panic!("expected section input"),
        }
        press(&mut engine, KeyCode::Esc);

        engine.app.left_state.select(Some(0));
        press(&mut engine, KeyCode::Char('A'));
        assert!(matches!(engine.app.mode, Mode::InputSection(_)));
        press(&mut engine, KeyCode::Esc);

        engine.app.left_state.select(Some(0));
        press(&mut engine, KeyCode::Char('i'));
        assert!(matches!(engine.app.mode, Mode::InputSection(_)));
        press(&mut engine, KeyCode::Enter);
        press(&mut engine, KeyCode::Char('d'));
        press(&mut engine, KeyCode::Char('d'));
        assert_eq!(engine.todo_list.sections.len(), 1);
    }

    #[test]
    fn task_creation_keys_choose_insert_position() {
        let (_dir, mut engine) = engine("# main\n- [ ] A\n- [ ] B\n");
        engine.app.focus = Focus::Right;
        for (key, index, above) in [
            ('a', Some(1), false),
            ('o', Some(1), false),
            ('O', Some(0), true),
            ('A', None, false),
        ] {
            engine.app.right_state.select(Some(0));
            press(&mut engine, KeyCode::Char(key));
            match &engine.app.mode {
                Mode::InputTask(input) => {
                    assert_eq!(input.insert_idx, index, "key {key}");
                    assert_eq!(input.above, above, "key {key}");
                }
                _ => panic!("expected task input for {key}"),
            }
            press(&mut engine, KeyCode::Esc);
        }
    }

    #[test]
    fn ctrl_c_quits_like_q() {
        let (_dir, mut engine1) = engine("# main\n- [ ] A\n");
        assert_eq!(
            engine1.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Control::Quit
        );
        let (_dir, mut engine2) = engine("# main\n- [ ] A\n");
        assert_eq!(press(&mut engine2, KeyCode::Char('q')), Control::Quit);
    }
}
