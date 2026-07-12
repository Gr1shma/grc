use crate::task::{NodePath, Section, Task};
use ratatui::widgets::ListState;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Focus {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeItem {
    Node(NodePath),
    Ghost(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardItem {
    Task(Task),
    Section(Section),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Help,

    InputTask {
        editing_idx: Option<usize>,
        insert_idx: Option<usize>,
        above: bool,
        buf: String,
        cursor: usize,
    },

    InputDue {
        task_idx: usize,
        buf: String,
        cursor: usize,
    },

    InputSection {
        node: Option<NodePath>,
        parent: Option<NodePath>,
        insert_idx: Option<usize>,
        buf: String,
        cursor: usize,
    },
}

pub struct AppState {
    pub focus: Focus,
    pub mode: Mode,
    pub tree_items: Vec<TreeItem>,
    pub left_state: ListState,
    pub right_state: ListState,

    pub pending_d: bool,

    pub pending_g: bool,

    pub pending_y: bool,

    pub clipboard: Option<ClipboardItem>,
    pub help_scroll: u16,
}

impl AppState {
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
            pending_d: false,
            pending_g: false,
            pending_y: false,
            clipboard: None,
            help_scroll: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_with_zero_sections_no_left_selection() {
        let app = AppState::new(0);
        assert!(app.left_state.selected().is_none());
    }

    #[test]
    fn new_with_sections_selects_first() {
        let app = AppState::new(3);
        assert_eq!(app.left_state.selected(), Some(0));
    }

    #[test]
    fn new_right_state_selects_first() {
        let app = AppState::new(1);
        assert_eq!(app.right_state.selected(), Some(0));
    }

    #[test]
    fn default_mode_is_normal() {
        let app = AppState::new(1);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn default_focus_is_left() {
        let app = AppState::new(1);
        assert_eq!(app.focus, Focus::Left);
    }

    #[test]
    fn pending_keys_are_initially_false() {
        let app = AppState::new(1);
        assert!(!app.pending_d);
        assert!(!app.pending_g);
    }

    #[test]
    fn tree_items_starts_empty() {
        let app = AppState::new(2);
        assert!(app.tree_items.is_empty());
    }

    #[test]
    fn focus_variants_are_distinct() {
        assert_ne!(Focus::Left, Focus::Right);
    }

    #[test]
    fn tree_item_node_equality() {
        assert_eq!(TreeItem::Node(vec![0]), TreeItem::Node(vec![0]));
        assert_ne!(TreeItem::Node(vec![0]), TreeItem::Node(vec![1]));
    }

    #[test]
    fn tree_item_node_vs_ghost() {
        assert_ne!(TreeItem::Node(vec![0]), TreeItem::Ghost(0));
    }
}
