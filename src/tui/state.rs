//! Application state and the input modes.
//!
//! State is deliberately separate from rendering and from the Markdown model.
//! Input modes own their edit target and text cursor, so the event loop never
//! has to copy buffers in and out of the mode on every keystroke.

use crate::task::NodePath;
use crate::task::Section;
use crate::task::Task;
use crate::tui::task_ref::TaskRef;
use chrono::{Local, NaiveDate};
use ratatui::widgets::ListState;
use std::collections::HashSet;
use std::time::{Duration, Instant};

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

/// Two-key command prefixes (`dd`, `gg`, `yy`, `z…`, `]d`).
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
    /// `z` was pressed; the next key folds or unfolds.
    Fold,
    /// `]` was pressed; `d` jumps to the next overdue task.
    Overdue,
}

/// How long a status message stays on screen.
pub const MESSAGE_TTL: Duration = Duration::from_secs(3);

/// Severity of a transient status-bar message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    /// Neutral information.
    Info,
    /// A completed action.
    Success,
    /// A save/conflict/input error.
    Error,
}

/// A short message shown in the status bar until the next key or its timeout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Human-readable text.
    pub text: String,
    /// Styling/severity.
    pub kind: MessageKind,
}

impl Message {
    /// Informational message.
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: MessageKind::Info,
        }
    }

    /// Successful-action message.
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: MessageKind::Success,
        }
    }

    /// Error message.
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: MessageKind::Error,
        }
    }
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
    /// The file changed on disk; wait for reload/overwrite.
    Conflict,
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
    /// Numeric motion/command prefix.
    pub count: Option<usize>,
    /// Sections whose children are hidden.
    pub folded: HashSet<NodePath>,
    /// Whether completed tasks are hidden from the task panel.
    pub hide_done: bool,
    /// Date used for due-date parsing and display.
    pub today: NaiveDate,
    /// Transient message shown on the next rendered status bar.
    pub message: Option<Message>,
    /// When `message` should disappear. No thread is involved: the event loop
    /// sleeps in `poll` until exactly this deadline (or until a key arrives)
    /// and then clears the message, so an idle screen costs no wake-ups.
    message_deadline: Option<Instant>,
    /// Last time the event loop told us. Injected rather than read so the
    /// expiry rules can be tested without sleeping.
    clock: Instant,
    /// Last known terminal height, used for bounded help scrolling.
    pub viewport_height: u16,
}

impl AppState {
    /// Build state for a document with `section_count` top-level sections.
    pub fn new(section_count: usize) -> Self {
        Self::new_with_today(section_count, Local::now().date_naive())
    }

    /// Build state with an injected date for deterministic tests.
    pub fn new_with_today(section_count: usize, today: NaiveDate) -> Self {
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
            count: None,
            folded: HashSet::new(),
            hide_done: false,
            today,
            message: None,
            message_deadline: None,
            clock: Instant::now(),
            viewport_height: 24,
        }
    }

    /// Clear any pending two-key command and its count prefix.
    pub fn clear_pending(&mut self) {
        self.pending = PendingKey::None;
        self.count = None;
    }

    /// Append a digit to the count prefix.
    pub fn push_count_digit(&mut self, digit: usize) {
        if self.count.is_none() && digit == 0 {
            return;
        }
        let current = self.count.unwrap_or(0);
        self.count = Some(current.saturating_mul(10).saturating_add(digit).min(9999));
    }

    /// Take the count prefix, defaulting to one.
    pub fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1).max(1)
    }

    /// Show an informational message.
    pub fn set_info(&mut self, text: impl Into<String>) {
        self.set_message(Message::info(text));
    }

    /// Show a success message.
    pub fn set_success(&mut self, text: impl Into<String>) {
        self.set_message(Message::success(text));
    }

    /// Show an error message.
    pub fn set_error(&mut self, text: impl Into<String>) {
        self.set_message(Message::error(text));
    }

    /// Show a message, restarting its countdown.
    pub fn set_message(&mut self, message: Message) {
        self.message = Some(message);
        self.message_deadline = Some(self.clock + MESSAGE_TTL);
    }

    /// Clear the current status message.
    pub fn clear_message(&mut self) {
        self.message = None;
        self.message_deadline = None;
    }

    /// How long until the showing message expires.
    ///
    /// `None` when nothing is showing; the event loop then blocks until input
    /// arrives instead of waking up to check a timer.
    pub fn message_timeout(&self) -> Option<Duration> {
        self.message_deadline
            .map(|deadline| deadline.saturating_duration_since(self.clock))
    }

    /// Move the clock to `now`, clearing the message if its time is up.
    ///
    /// Returns whether the message disappeared, so the caller can repaint only
    /// when there is something new to show.
    pub fn advance(&mut self, now: Instant) -> bool {
        self.clock = now;
        if self
            .message_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.clear_message();
            return true;
        }
        false
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

    #[test]
    fn count_prefix_accumulates_and_is_taken_once() {
        let mut app = AppState::new(1);
        app.push_count_digit(0);
        assert_eq!(app.count, None);
        app.push_count_digit(1);
        app.push_count_digit(2);
        assert_eq!(app.take_count(), 12);
        assert_eq!(app.take_count(), 1);
    }

    #[test]
    fn messages_are_typed_and_clearable() {
        let mut app = AppState::new(0);
        app.set_error("save failed");
        assert_eq!(app.message.as_ref().unwrap().kind, MessageKind::Error);
        app.clear_message();
        assert!(app.message.is_none());
    }

    #[test]
    fn a_message_expires_after_its_ttl() {
        let start = Instant::now();
        let mut app = AppState::new(0);
        app.advance(start);
        app.set_info("saved");
        assert_eq!(app.message_timeout(), Some(MESSAGE_TTL));

        assert!(
            !app.advance(start + MESSAGE_TTL - Duration::from_millis(1)),
            "the message is still on screen"
        );
        assert!(app.message.is_some());
        assert!(app.advance(start + MESSAGE_TTL), "the deadline clears it");
        assert!(app.message.is_none());
        assert_eq!(app.message_timeout(), None);
        assert!(!app.advance(start + MESSAGE_TTL), "nothing left to clear");
    }

    #[test]
    fn a_new_message_restarts_the_countdown() {
        let start = Instant::now();
        let mut app = AppState::new(0);
        app.advance(start);
        app.set_error("save failed");
        app.advance(start + MESSAGE_TTL - Duration::from_millis(1));
        app.set_error("save failed again");
        assert!(!app.advance(start + MESSAGE_TTL));
        assert!(app.message.is_some(), "the countdown started over");
        assert!(app.advance(start + 2 * MESSAGE_TTL), "and ends on its own");
    }

    #[test]
    fn nothing_on_screen_means_no_deadline() {
        let mut app = AppState::new(0);
        app.advance(Instant::now());
        assert_eq!(app.message_timeout(), None);
        app.set_info("saved");
        app.clear_message();
        assert_eq!(app.message_timeout(), None);
        // A clock that went backwards must not produce a panic or a negative
        // wait; the message simply stays until it is due.
        app.set_info("saved");
        let earlier = Instant::now() - Duration::from_secs(1);
        app.advance(earlier);
        assert!(app.message_timeout().is_some_and(|wait| wait > MESSAGE_TTL));
    }

    #[test]
    fn injected_today_is_used_by_state() {
        let today = NaiveDate::from_ymd_opt(2025, 7, 21).unwrap();
        let app = AppState::new_with_today(0, today);
        assert_eq!(app.today, today);
    }
}
