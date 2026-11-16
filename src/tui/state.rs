//! Application state and the input modes.
//!
//! State is deliberately separate from rendering and from the Markdown model.
//! Input modes own their edit target and text cursor, so the event loop never
//! has to copy buffers in and out of the mode on every keystroke.

use crate::task::NodePath;
use crate::task::Section;
use crate::task::Task;
use crate::tui::task_ref::TaskRef;
use ratatui::widgets::ListState;

/// Which panel receives navigation keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// The section tree on the left.
    Left,
    /// The task list on the right.
    Right,
}

/// A row in the flattened section tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeItem {
    /// A real section at this path.
    Node(NodePath),
    /// A temporary row shown while typing a new section.
    Ghost(usize),
}

/// What `yy`/`dd` placed on the clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardItem {
    /// A single task.
    Task(Task),
    /// A section together with all of its descendants.
    Section(Section),
}

/// Two-key command prefixes (`dd`, `gg`, `yy`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PendingKey {
    /// No prefix is active.
    #[default]
    None,
    /// `d` was pressed; another `d` deletes.
    Delete,
    /// `g` was pressed; another `g` jumps to the top.
    Goto,
    /// `y` was pressed; another `y` yanks.
    Yank,
}

/// An editable single-line buffer.
///
/// `cursor` is a character index, not a byte index, so multi-byte text is safe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextField {
    /// Current text.
    pub buf: String,
    /// Character index of the insertion point.
    pub cursor: usize,
}

impl TextField {
    /// Create a field with the cursor at the end of `text`.
    pub fn new(text: impl Into<String>) -> Self {
        let buf = text.into();
        let cursor = buf.chars().count();
        Self { buf, cursor }
    }

    /// Move the cursor to the end.
    pub fn move_to_end(&mut self) {
        self.cursor = self.buf.chars().count();
    }
}

/// New-task input state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskInput {
    /// Task being renamed, resolved through the current filtered list.
    pub editing_idx: Option<usize>,
    /// Filtered-list position used for the ghost row and selection.
    pub insert_idx: Option<usize>,
    /// Insert above the anchor rather than below it.
    pub above: bool,
    /// Section selected when input started.
    pub node: NodePath,
    /// Text being edited.
    pub field: TextField,
}

/// Due-date input state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueInput {
    /// Actual task being edited, independent of filter positions.
    pub target: TaskRef,
    /// Date expression being edited.
    pub field: TextField,
}

/// New-section or rename-section input state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionInput {
    /// Existing section to rename, if any.
    pub node: Option<NodePath>,
    /// Parent for a new section, or `None` for the document root.
    pub parent: Option<NodePath>,
    /// Insertion index among the parent's children.
    pub insert_idx: Option<usize>,
    /// Heading level for a new section.
    pub level: usize,
    /// Heading text being edited.
    pub field: TextField,
}

/// Current input mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Normal navigation.
    Normal,
    /// Help overlay.
    Help,
    /// Live filter editing.
    Filter,
    /// Creating or renaming a task.
    InputTask(TaskInput),
    /// Editing a task's due date.
    InputDue(DueInput),
    /// Creating or renaming a section.
    InputSection(SectionInput),
}

impl Mode {
    /// Whether this mode captures text instead of navigation keys.
    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            Self::Filter | Self::InputTask(_) | Self::InputDue(_) | Self::InputSection(_)
        )
    }

    /// Whether this mode draws a section ghost row.
    pub fn is_section_input(&self) -> bool {
        matches!(self, Self::InputSection(_))
    }
}

/// Selection to restore after a filter is cleared.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionSnapshot {
    /// Selected section path.
    pub node: Option<NodePath>,
    /// Selected task, when the task panel had focus.
    pub task: Option<TaskRef>,
}

/// All ephemeral UI state.
pub struct AppState {
    /// Active panel.
    pub focus: Focus,
    /// Active input mode.
    pub mode: Mode,
    /// Flattened rows matching the current filter and mode.
    pub tree_items: Vec<TreeItem>,
    /// Section-list selection.
    pub left_state: ListState,
    /// Task-list selection.
    pub right_state: ListState,
    /// Pending two-key command.
    pub pending: PendingKey,
    /// Last yanked or deleted item.
    pub clipboard: Option<ClipboardItem>,
    /// Help overlay scroll offset.
    pub help_scroll: u16,
    /// Raw filter text.
    pub filter: String,
    /// Lowercased filter text used for matching.
    pub filter_lower: String,
    /// Section to keep selected while the filter text changes.
    pub filter_anchor: Option<NodePath>,
    /// Selection to restore when the filter is cleared.
    pub filter_restore: Option<SelectionSnapshot>,
}

impl AppState {
    /// Build state for a document with `section_count` top-level sections.
    pub fn new(section_count: usize) -> Self {
        let mut left_state = ListState::default();
        if section_count > 0 {
            left_state.select(Some(0));
        }
        let mut right_state = ListState::default();
        right_state.select(Some(0));

        Self {
            focus: Focus::Left,
            mode: Mode::Normal,
            tree_items: Vec::new(),
            left_state,
            right_state,
            pending: PendingKey::None,
            clipboard: None,
            help_scroll: 0,
            filter: String::new(),
            filter_lower: String::new(),
            filter_anchor: None,
            filter_restore: None,
        }
    }

    /// Clear any pending two-key command.
    pub fn clear_pending(&mut self) {
        self.pending = PendingKey::None;
    }

    /// Clear the active filter.
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.filter_lower.clear();
    }

    /// Replace the filter text and keep the lowercase copy in sync.
    pub fn set_filter(&mut self, filter: String) {
        self.filter_lower = filter.to_lowercase();
        self.filter = filter;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_with_zero_sections_has_no_left_selection() {
        let app = AppState::new(0);
        assert!(app.left_state.selected().is_none());
    }

    #[test]
    fn new_with_sections_selects_first() {
        let app = AppState::new(3);
        assert_eq!(app.left_state.selected(), Some(0));
        assert_eq!(app.focus, Focus::Left);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.pending, PendingKey::None);
    }

    #[test]
    fn text_field_cursor_is_char_based() {
        let field = TextField::new("héllo日本");
        assert_eq!(field.cursor, 7);
        assert_eq!(field.buf.chars().count(), 7);
    }

    #[test]
    fn set_filter_keeps_lowercase_copy() {
        let mut app = AppState::new(1);
        app.set_filter("Mixed Case".into());
        assert_eq!(app.filter, "Mixed Case");
        assert_eq!(app.filter_lower, "mixed case");
        app.clear_filter();
        assert!(app.filter.is_empty());
        assert!(app.filter_lower.is_empty());
    }
}
