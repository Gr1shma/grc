//! Central palette, text styles, and terminal glyphs.
//!
//! Every color used by the TUI lives here. Render modules select semantic
//! styles from this struct instead of hardcoding colors. Named ANSI colors are
//! used deliberately so the UI still works on 16-color terminals.

use crate::tui::state::MessageKind;
use ratatui::style::{Color, Modifier, Style};

/// Environment variable that switches the UI to ASCII glyphs.
pub const ASCII_ENV: &str = "GRC_ASCII";

/// Standard colour opt-out environment variable.
pub const NO_COLOR_ENV: &str = "NO_COLOR";

/// Box, fold, and cursor glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyphs {
    /// Unchecked task box.
    pub checkbox_todo: &'static str,
    /// Completed task box.
    pub checkbox_done: &'static str,
    /// Expanded section marker.
    pub fold_open: &'static str,
    /// Collapsed section marker.
    pub fold_closed: &'static str,
    /// Vertical guide connecting a nested section to its parent.
    pub tree_guide: &'static str,
    /// Inline edit cursor.
    pub cursor: &'static str,
    /// Separator between a task and its child-section breadcrumb.
    pub breadcrumb_sep: &'static str,
    /// Indicator for an invalid due token.
    pub invalid_due: &'static str,
    /// Arrow used before a resolved inline-date preview.
    pub preview: &'static str,
    /// Truncation marker.
    pub ellipsis: &'static str,
    /// Scrollbar track.
    pub scroll_track: &'static str,
    /// Scrollbar thumb.
    pub scroll_thumb: &'static str,
}

impl Glyphs {
    /// Unicode box drawing and symbols.
    pub const fn unicode() -> Self {
        Self {
            checkbox_todo: "☐",
            checkbox_done: "☑",
            fold_open: "▾",
            fold_closed: "▸",
            tree_guide: "│",
            cursor: "│",
            breadcrumb_sep: "›",
            invalid_due: "!",
            preview: "→",
            ellipsis: "…",
            scroll_track: "│",
            scroll_thumb: "█",
        }
    }

    /// ASCII-only fallback for limited terminals.
    pub const fn ascii() -> Self {
        Self {
            checkbox_todo: "[ ]",
            checkbox_done: "[x]",
            fold_open: "v",
            fold_closed: ">",
            tree_guide: "|",
            cursor: "|",
            breadcrumb_sep: ">",
            invalid_due: "!",
            preview: "->",
            ellipsis: "...",
            scroll_track: "|",
            scroll_thumb: "#",
        }
    }
}

/// Semantic mode-pill kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModePill {
    /// Normal navigation.
    Normal,
    /// Any text-editing mode.
    Edit,
    /// Live filter.
    Filter,
    /// Help overlay.
    Help,
    /// External-change conflict.
    Conflict,
}

/// Semantic due-date groups used for colour selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DueUrgency {
    /// The task is completed.
    Done,
    /// The date is before today.
    Overdue,
    /// The date is today.
    Today,
    /// The date is within the next three days.
    Soon,
    /// The date is further away.
    Later,
}

/// All styles used by the TUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// Whether colour is enabled.
    pub color: bool,
    /// Primary accent.
    pub accent: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text.
    pub dim: Color,
    /// Error/destructive text.
    pub error: Color,
    /// Warning text.
    pub warning: Color,
    /// Success text.
    pub success: Color,
    /// Selection bar background.
    pub selection: Color,
    /// Glyph set in use.
    pub glyphs: Glyphs,
}

impl Theme {
    /// Build a theme from `NO_COLOR` and `GRC_ASCII`.
    pub fn from_env() -> Self {
        let no_color = std::env::var_os(NO_COLOR_ENV).is_some_and(|value| !value.is_empty());
        let ascii =
            std::env::var_os(ASCII_ENV).is_some_and(|value| !value.is_empty() && value != "0");
        Self::new(!no_color, ascii)
    }

    /// Deterministic colourless theme for render tests.
    pub const fn plain() -> Self {
        Self::new(false, false)
    }

    /// Construct a theme with explicit capabilities.
    pub const fn new(color: bool, ascii: bool) -> Self {
        let reset = Color::Reset;
        Self {
            color,
            accent: if color { Color::Cyan } else { reset },
            text: reset,
            dim: if color { Color::DarkGray } else { reset },
            error: if color { Color::Red } else { reset },
            warning: if color { Color::Yellow } else { reset },
            success: if color { Color::Green } else { reset },
            selection: if color { Color::DarkGray } else { reset },
            glyphs: if ascii {
                Glyphs::ascii()
            } else {
                Glyphs::unicode()
            },
        }
    }

    /// Focused panel border: accent; unfocused: dim.
    pub fn border(&self, focused: bool) -> Style {
        if focused {
            Style::default().fg(self.accent)
        } else {
            Style::default().fg(self.dim)
        }
    }

    /// Focused panel title is bold, unfocused is dim.
    pub fn title(&self, focused: bool) -> Style {
        let style = Style::default().fg(if focused { self.accent } else { self.dim });
        if focused {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    /// Subtle selected-row bar. Colourless terminals use an underline instead
    /// of a full reverse-video bar.
    pub fn selection_style(&self) -> Style {
        if self.color {
            Style::default().bg(self.selection)
        } else {
            Style::default().add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
        }
    }

    /// Ordinary body text.
    pub fn text_style(&self) -> Style {
        Style::default().fg(self.text)
    }

    /// Secondary, low-emphasis text.
    pub fn dim_style(&self) -> Style {
        Style::default().fg(self.dim)
    }

    /// Completed-task text.
    pub fn done_style(&self) -> Style {
        Style::default()
            .fg(self.dim)
            .add_modifier(Modifier::CROSSED_OUT)
    }

    /// Section name in the tree. Top-level sections are emphasised; nested
    /// sections stay plain so depth reads through the guides instead.
    pub fn section_name_style(&self, depth: usize) -> Style {
        if depth == 0 {
            self.text_style().add_modifier(Modifier::BOLD)
        } else {
            self.text_style()
        }
    }

    /// Tree guide bars drawn to the left of nested sections.
    pub fn tree_guide(&self) -> Style {
        Style::default().fg(self.dim)
    }

    /// Progress counters and other quiet metadata.
    pub fn meta_style(&self) -> Style {
        Style::default().fg(self.dim)
    }

    /// Checkbox glyph and style for a task.
    pub fn checkbox(&self, done: bool) -> (&'static str, Style) {
        if done {
            (self.glyphs.checkbox_done, Style::default().fg(self.success))
        } else {
            (self.glyphs.checkbox_todo, Style::default().fg(self.dim))
        }
    }

    /// Fold marker style for the section tree.
    pub fn fold_style(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Due-date colour for a semantic urgency group.
    pub fn due_style(&self, urgency: DueUrgency) -> Style {
        match urgency {
            DueUrgency::Done => self.dim_style(),
            DueUrgency::Overdue => Style::default().fg(self.error).add_modifier(Modifier::BOLD),
            DueUrgency::Today => Style::default()
                .fg(self.warning)
                .add_modifier(Modifier::BOLD),
            DueUrgency::Soon => Style::default().fg(self.accent),
            DueUrgency::Later => self.dim_style(),
        }
    }

    /// Coloured mode pill.
    pub fn mode_pill(&self, pill: ModePill) -> Style {
        if !self.color {
            return Style::default().add_modifier(Modifier::BOLD);
        }
        let background = match pill {
            ModePill::Normal => self.accent,
            ModePill::Edit => self.success,
            ModePill::Filter => self.warning,
            ModePill::Help => self.accent,
            ModePill::Conflict => self.error,
        };
        Style::default()
            .fg(Color::Black)
            .bg(background)
            .add_modifier(Modifier::BOLD)
    }

    /// Transient status message colour.
    pub fn message_style(&self, kind: MessageKind) -> Style {
        match kind {
            MessageKind::Info => self.dim_style(),
            MessageKind::Success => Style::default().fg(self.success),
            MessageKind::Error => Style::default().fg(self.error),
        }
    }

    /// Key names in the hint line.
    pub fn key_style(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Hint descriptions.
    pub fn hint_style(&self) -> Style {
        Style::default().fg(self.dim)
    }

    /// Filter-match emphasis.
    pub fn match_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Inline edit cursor.
    pub fn cursor_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Popup border.
    pub fn dialog_border(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Popup title.
    pub fn dialog_title(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Help-section heading.
    pub fn help_heading(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Help key names.
    pub fn help_key(&self) -> Style {
        Style::default().fg(self.text)
    }

    /// Help descriptions.
    pub fn help_text(&self) -> Style {
        Style::default().fg(self.dim)
    }

    /// Scrollbar thumb.
    pub fn scrollbar_thumb(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Scrollbar track.
    pub fn scrollbar_track(&self) -> Style {
        Style::default().fg(self.dim)
    }

    /// Warning text for orphan tasks and conflicts.
    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    /// Error text for invalid input.
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }
}
