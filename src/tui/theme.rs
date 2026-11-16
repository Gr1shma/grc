//! Central palette, text styles, and terminal glyphs.
//!
//! Every colour used by the TUI lives here. Render modules select semantic
//! styles from this struct instead of hardcoding colours.
//!
//! The palette is explicit RGB rather than named ANSI colours. The selected row
//! is a tint a few shades lighter than the panel background, and the 16-colour
//! names have no shade between "black" and "dark gray" to express that; naming
//! the selection with a colour also means the text drawn on top of it can be
//! chosen and *tested* against it. Every foreground here is checked against the
//! background it can land on by `every_row_style_clears_wcag_contrast`.
//!
//! Terminals that cannot show colour set `NO_COLOR=1`, which drops back to the
//! terminal's own colours; selection is then carried by a glyph in the gutter
//! plus bold text, so it never depends on colour alone.

use crate::tui::state::MessageKind;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Environment variable that switches the UI to ASCII glyphs.
pub const ASCII_ENV: &str = "GRC_ASCII";

/// Standard colour opt-out environment variable.
pub const NO_COLOR_ENV: &str = "NO_COLOR";

/// Panel canvas behind every row.
const BG: Color = Color::Rgb(16, 20, 26);
/// Selection bar of the focused panel.
const SELECTION: Color = Color::Rgb(38, 44, 54);
/// Selection bar of the unfocused panel: weaker, but still clearly a bar.
const SELECTION_UNFOCUSED: Color = Color::Rgb(27, 32, 40);
/// Ordinary body text.
const TEXT: Color = Color::Rgb(226, 232, 240);
/// Secondary text on the panel canvas.
const DIM: Color = Color::Rgb(148, 163, 184);
/// Secondary text on a selection bar. Deliberately brighter than [`DIM`]:
/// dim gray on a gray bar is exactly the combination that was unreadable.
const DIM_SELECTED: Color = Color::Rgb(176, 190, 206);
/// Primary accent.
const ACCENT: Color = Color::Rgb(125, 211, 252);
/// Muted accent for secondary breadcrumbs.
const ACCENT_DIM: Color = Color::Rgb(103, 176, 222);
/// Error/destructive text.
const ERROR: Color = Color::Rgb(248, 113, 113);
/// Warning text.
const WARNING: Color = Color::Rgb(250, 204, 21);
/// Success text.
const SUCCESS: Color = Color::Rgb(74, 222, 128);
/// Decorative tree guides; only ever a couple of cells wide.
const GUIDE: Color = Color::Rgb(120, 138, 162);

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
    /// Bar drawn in the gutter of the selected row.
    pub selection_marker: &'static str,
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
            selection_marker: "▌",
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
            selection_marker: "|",
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
    /// Panel canvas.
    pub bg: Color,
    /// Selection bar of the focused panel.
    pub selection: Color,
    /// Selection bar of the unfocused panel.
    pub selection_unfocused: Color,
    /// Primary accent.
    pub accent: Color,
    /// Muted accent (breadcrumbs, unfocused marker).
    pub accent_dim: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text on the canvas.
    pub dim: Color,
    /// Secondary text on a selection bar.
    pub dim_selected: Color,
    /// Error/destructive text.
    pub error: Color,
    /// Warning text.
    pub warning: Color,
    /// Success text.
    pub success: Color,
    /// Decorative tree guides.
    pub guide: Color,
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
            bg: if color { BG } else { reset },
            selection: if color { SELECTION } else { reset },
            selection_unfocused: if color { SELECTION_UNFOCUSED } else { reset },
            accent: if color { ACCENT } else { reset },
            accent_dim: if color { ACCENT_DIM } else { reset },
            text: if color { TEXT } else { reset },
            dim: if color { DIM } else { reset },
            dim_selected: if color { DIM_SELECTED } else { reset },
            error: if color { ERROR } else { reset },
            warning: if color { WARNING } else { reset },
            success: if color { SUCCESS } else { reset },
            guide: if color { GUIDE } else { reset },
            glyphs: if ascii {
                Glyphs::ascii()
            } else {
                Glyphs::unicode()
            },
        }
    }

    /// Panel canvas: the background every row is drawn on.
    pub fn base_style(&self) -> Style {
        Style::default().fg(self.text).bg(self.bg)
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

    /// Background of a row, selected or not.
    ///
    /// The panel is painted with [`Theme::base_style`] first, so a row only has
    /// to name the bar when it is selected.
    pub fn row_background(&self, selected: bool, focused: bool) -> Color {
        if !selected {
            self.bg
        } else if focused {
            self.selection
        } else {
            self.selection_unfocused
        }
    }

    /// The selected-row bar. This style carries the background only, so the
    /// foreground each span chose for the bar survives.
    ///
    /// Without colour there is no bar to draw, so the row is emphasised with
    /// reverse video (focused) or bold, and the gutter marker does the rest.
    pub fn selection_style(&self, focused: bool) -> Style {
        if self.color {
            Style::default().bg(self.row_background(true, focused))
        } else if focused {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
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

    /// Progress counters and other quiet metadata.
    pub fn meta_style(&self) -> Style {
        Style::default().fg(self.dim)
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
}

/// The palette a single list row needs.
///
/// A row can sit under a selection bar, where the secondary colours that read
/// well on the panel canvas lose most of their contrast. Every row colour is
/// looked up through this type so the selected and unselected cases cannot be
/// mixed up, and so a test can walk exactly the colours a row can use.
#[derive(Clone, Copy, Debug)]
pub struct RowTheme<'a> {
    theme: &'a Theme,
    selected: bool,
    focused: bool,
}

impl<'a> RowTheme<'a> {
    /// Styles for one row of a panel.
    pub fn new(theme: &'a Theme, selected: bool, focused: bool) -> Self {
        Self {
            theme,
            selected,
            focused,
        }
    }

    /// Glyph set; the same in every row.
    pub fn glyphs(&self) -> &'a Glyphs {
        &self.theme.glyphs
    }

    /// Secondary colour for this row's background.
    fn secondary(&self) -> Color {
        if self.selected {
            self.theme.dim_selected
        } else {
            self.theme.dim
        }
    }

    /// Task or section name.
    pub fn text(&self) -> Style {
        Style::default().fg(self.theme.text)
    }

    /// Quiet metadata: progress counters, dates without urgency.
    pub fn dim(&self) -> Style {
        Style::default().fg(self.secondary())
    }

    /// Completed-task text: quiet and struck through.
    pub fn done(&self) -> Style {
        self.dim().add_modifier(Modifier::CROSSED_OUT)
    }

    /// Section name in the tree. Top-level sections are emphasised; nested
    /// sections stay plain so depth reads through the guides instead.
    pub fn section_name(&self, depth: usize) -> Style {
        if depth == 0 {
            self.text().add_modifier(Modifier::BOLD)
        } else {
            self.text()
        }
    }

    /// Tree guide bars drawn to the left of nested sections.
    pub fn guide(&self) -> Style {
        Style::default().fg(self.theme.guide)
    }

    /// Fold marker.
    pub fn fold(&self) -> Style {
        Style::default().fg(self.theme.accent)
    }

    /// Section breadcrumb in front of a task from a subsection.
    pub fn breadcrumb(&self) -> Style {
        Style::default().fg(self.theme.accent_dim)
    }

    /// Filter-match emphasis inside row text.
    pub fn matched(&self) -> Style {
        Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Inline edit cursor.
    pub fn cursor(&self) -> Style {
        Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Text that the user has typed but that cannot be accepted.
    pub fn error(&self) -> Style {
        Style::default().fg(self.theme.error)
    }

    /// Checkbox glyph and style for a task.
    pub fn checkbox(&self, done: bool) -> (&'static str, Style) {
        if done {
            (
                self.theme.glyphs.checkbox_done,
                Style::default().fg(self.theme.success),
            )
        } else {
            (
                self.theme.glyphs.checkbox_todo,
                Style::default().fg(self.secondary()),
            )
        }
    }

    /// Due-date colour for a semantic urgency group.
    pub fn due(&self, urgency: DueUrgency) -> Style {
        match urgency {
            DueUrgency::Done | DueUrgency::Later => self.dim(),
            DueUrgency::Overdue => Style::default()
                .fg(self.theme.error)
                .add_modifier(Modifier::BOLD),
            DueUrgency::Today => Style::default()
                .fg(self.theme.warning)
                .add_modifier(Modifier::BOLD),
            DueUrgency::Soon => Style::default().fg(self.theme.accent),
        }
    }

    /// Style of the gutter marker.
    pub fn marker_style(&self) -> Style {
        Style::default()
            .fg(if self.focused {
                self.theme.accent
            } else {
                self.theme.accent_dim
            })
            .add_modifier(Modifier::BOLD)
    }

    /// The single gutter cell in front of a row: a bar when selected, a blank
    /// otherwise. This is what keeps the selection visible on terminals that
    /// cannot render the background tint.
    pub fn gutter(&self) -> Span<'static> {
        if self.selected {
            Span::styled(
                self.theme.glyphs.selection_marker.to_string(),
                self.marker_style(),
            )
        } else {
            Span::raw(" ")
        }
    }
}

/// Contrast measurement shared by the palette tests and the render tests.
///
/// Kept out of the non-test build: it exists so that the *drawn* cells can be
/// checked, not just the palette they came from.
#[cfg(test)]
pub(crate) mod contrast {
    use ratatui::style::Color;

    /// WCAG 2.1 minimum for body text.
    pub(crate) const TEXT_MIN: f64 = 4.5;
    /// WCAG 2.1 minimum for large text and non-text decoration.
    pub(crate) const DECOR_MIN: f64 = 3.0;

    fn channel(value: u8) -> f64 {
        let value = f64::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    /// sRGB values for the 16 standard ANSI colours (xterm defaults).
    ///
    /// The palette itself is RGB, but the colours it replaced were named, and
    /// naming them here is what lets the regression test quote a number.
    pub(crate) fn sample(color: Color) -> (u8, u8, u8) {
        match color {
            Color::Rgb(red, green, blue) => (red, green, blue),
            Color::Black => (0, 0, 0),
            Color::Red => (205, 0, 0),
            Color::Green => (0, 205, 0),
            Color::Yellow => (205, 205, 0),
            Color::Blue => (0, 0, 238),
            Color::Magenta => (205, 0, 205),
            Color::Cyan => (0, 205, 205),
            Color::Gray => (229, 229, 229),
            Color::DarkGray => (127, 127, 127),
            Color::LightRed => (255, 0, 0),
            Color::LightGreen => (0, 255, 0),
            Color::LightYellow => (255, 255, 0),
            Color::LightBlue => (92, 92, 255),
            Color::LightMagenta => (255, 0, 255),
            Color::LightCyan => (0, 255, 255),
            Color::White => (255, 255, 255),
            other => panic!("no sRGB sample for {other:?}"),
        }
    }

    pub(crate) fn luminance(color: Color) -> f64 {
        let (red, green, blue) = sample(color);
        0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
    }

    /// WCAG 2.1 contrast ratio between two colours.
    pub(crate) fn ratio(foreground: Color, background: Color) -> f64 {
        let (lighter, darker) = {
            let (a, b) = (luminance(foreground), luminance(background));
            if a > b { (a, b) } else { (b, a) }
        };
        (lighter + 0.05) / (darker + 0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::contrast::{DECOR_MIN, TEXT_MIN, luminance, ratio as contrast};
    use super::*;

    fn assert_contrast(what: &str, style: Style, background: Color, minimum: f64) {
        let foreground = style.fg.expect("every row colour sets an explicit fg");
        let ratio = contrast(foreground, background);
        assert!(
            ratio >= minimum,
            "{what}: {foreground:?} on {background:?} is {ratio:.2}:1, needs {minimum}:1"
        );
    }
    /// Every combination of (selected, focused) a panel can draw.
    const ROWS: [(bool, bool); 3] = [(false, false), (true, true), (true, false)];

    #[test]
    fn every_row_style_clears_wcag_contrast() {
        let theme = Theme::new(true, false);
        for (selected, focused) in ROWS {
            let row = RowTheme::new(&theme, selected, focused);
            let background = theme.row_background(selected, focused);
            let label = format!("selected={selected} focused={focused}");
            assert_contrast(&format!("{label} text"), row.text(), background, TEXT_MIN);
            assert_contrast(&format!("{label} dim"), row.dim(), background, TEXT_MIN);
            assert_contrast(&format!("{label} done"), row.done(), background, TEXT_MIN);
            assert_contrast(
                &format!("{label} section name"),
                row.section_name(0),
                background,
                TEXT_MIN,
            );
            for done in [false, true] {
                assert_contrast(
                    &format!("{label} checkbox done={done}"),
                    row.checkbox(done).1,
                    background,
                    TEXT_MIN,
                );
            }
            for urgency in [
                DueUrgency::Done,
                DueUrgency::Overdue,
                DueUrgency::Today,
                DueUrgency::Soon,
                DueUrgency::Later,
            ] {
                assert_contrast(
                    &format!("{label} due {urgency:?}"),
                    row.due(urgency),
                    background,
                    TEXT_MIN,
                );
            }
            assert_contrast(
                &format!("{label} breadcrumb"),
                row.breadcrumb(),
                background,
                TEXT_MIN,
            );
            assert_contrast(
                &format!("{label} filter match"),
                row.matched(),
                background,
                TEXT_MIN,
            );
            assert_contrast(
                &format!("{label} cursor"),
                row.cursor(),
                background,
                TEXT_MIN,
            );
            assert_contrast(
                &format!("{label} invalid input"),
                row.error(),
                background,
                TEXT_MIN,
            );
            assert_contrast(
                &format!("{label} fold marker"),
                row.fold(),
                background,
                TEXT_MIN,
            );
            // Tree guides and the gutter marker are decoration, not text.
            assert_contrast(
                &format!("{label} guide"),
                row.guide(),
                background,
                DECOR_MIN,
            );
            assert_contrast(
                &format!("{label} marker"),
                row.marker_style(),
                background,
                DECOR_MIN,
            );
        }
    }

    /// The regression this palette exists to prevent: the old bar was
    /// `Color::DarkGray`, the same colour the secondary spans used, which left
    /// due dates, counters, and the breadcrumb prefix at 1:1 against it.
    #[test]
    fn the_old_gray_bar_would_fail_the_contrast_check() {
        assert!(contrast(Color::DarkGray, Color::DarkGray) < TEXT_MIN);
        // Even the brighter ANSI gray only ever reached ~3:1 on that bar, so
        // swapping one named gray for another would not have fixed it.
        assert!(contrast(Color::Gray, Color::DarkGray) < TEXT_MIN);
    }

    #[test]
    fn the_focused_bar_is_stronger_than_the_unfocused_one() {
        let theme = Theme::new(true, false);
        assert_ne!(theme.selection, theme.selection_unfocused);
        let focused = luminance(theme.selection);
        let unfocused = luminance(theme.selection_unfocused);
        let canvas = luminance(theme.bg);
        assert!(canvas < unfocused && unfocused < focused);
    }

    #[test]
    fn the_bar_is_drawn_whatever_the_colour_support() {
        let plain = Theme::plain();
        // No colour: an emphasis modifier plus the gutter glyph.
        let style = plain.selection_style(true);
        assert!(style.bg.is_none() && style.add_modifier.contains(Modifier::REVERSED));
        assert!(
            plain
                .selection_style(false)
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(plain.glyphs.selection_marker, "▌");

        let ascii = Theme::new(false, true);
        assert_eq!(ascii.glyphs.selection_marker, "|");
    }

    #[test]
    fn the_marker_fills_exactly_one_gutter_cell() {
        use crate::tui::widgets::{GUTTER, display_width};
        for glyphs in [Glyphs::unicode(), Glyphs::ascii()] {
            assert_eq!(display_width(glyphs.selection_marker), GUTTER);
        }
    }

    #[test]
    fn a_selected_row_never_reuses_the_canvas_secondary_colour() {
        let theme = Theme::new(true, false);
        let selected = RowTheme::new(&theme, true, true);
        let plain = RowTheme::new(&theme, false, false);
        assert_ne!(selected.dim().fg, plain.dim().fg);
        assert_ne!(selected.checkbox(false).1.fg, plain.checkbox(false).1.fg);
        for fg in [
            selected.dim().fg,
            selected.checkbox(false).1.fg,
            selected.due(DueUrgency::Later).fg,
            selected.done().fg,
        ] {
            assert_ne!(fg, Some(Color::DarkGray));
            assert_ne!(fg, Some(Color::Gray));
        }
    }
}
