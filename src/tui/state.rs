use crate::task::{Section, Subsection, Task};
use ratatui::widgets::ListState;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Focus {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TreeNode {
    Section(usize),
    Subsection(usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClipboardItem {
    Task(Task),
    Section(Section),
    Subsection(Subsection),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Normal,
    Help,

    InputTask {
        editing_idx: Option<usize>,
        insert_idx: Option<usize>,
        buf: String,
        cursor: usize,
    },

    InputDue {
        task_idx: usize,
        buf: String,
        cursor: usize,
    },

    InputSection {
        node: Option<TreeNode>,
        insert_idx: Option<usize>,
        buf: String,
        cursor: usize,
    },

    InputSubsection {
        parent_sec_idx: usize,
        insert_idx: Option<usize>,
        buf: String,
        cursor: usize,
    },
}

pub struct AppState {
    pub focus: Focus,
    pub mode: Mode,
    pub tree_nodes: Vec<TreeNode>,
    pub left_state: ListState,
    pub right_state: ListState,

    pub pending_d: bool,

    pub pending_g: bool,

    pub pending_y: bool,

    pub clipboard: Option<ClipboardItem>,
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
            tree_nodes: Vec::new(),
            left_state,
            right_state,
            pending_d: false,
            pending_g: false,
            pending_y: false,
            clipboard: None,
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
    fn tree_nodes_starts_empty() {
        let app = AppState::new(2);
        assert!(app.tree_nodes.is_empty());
    }

    #[test]
    fn focus_variants_are_distinct() {
        assert_ne!(Focus::Left, Focus::Right);
    }

    #[test]
    fn tree_node_section_equality() {
        assert_eq!(TreeNode::Section(0), TreeNode::Section(0));
        assert_ne!(TreeNode::Section(0), TreeNode::Section(1));
    }

    #[test]
    fn tree_node_subsection_equality() {
        assert_eq!(TreeNode::Subsection(0, 1), TreeNode::Subsection(0, 1));
        assert_ne!(TreeNode::Subsection(0, 1), TreeNode::Subsection(0, 2));
    }

    #[test]
    fn tree_node_section_vs_subsection() {
        assert_ne!(TreeNode::Section(0), TreeNode::Subsection(0, 0));
    }
}
