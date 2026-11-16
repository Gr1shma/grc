//! Central palette, text styles, and terminal glyphs.
//!
//! Every colour used by the TUI lives here. Render modules select semantic
//! styles from this struct instead of hardcoding colours.
//!
//! # Surfaces
//!
//! The interface has three kinds of surface and each has its own contract:
//!
//! * The **canvas** — the terminal's own background, unless `GRC_BG=1` asks us
//!   to paint [`BG`] across the screen. On a transparent canvas the primary and
//!   secondary row colours are left to the terminal (`Color::Reset`), so the
//!   pair the user chose in their terminal profile supplies the contrast. This
//!   is what makes the app usable on a light terminal without hardcoding a
//!   white palette.
//! * The **selection bar** — always our own colour, so every foreground drawn
//!   on it is chosen and tested here against that exact colour.
//! * **Popups** — always painted with our own canvas colour, so help text is
//!   never drawn over whatever the panel happened to show underneath.
//!
//! Coloured accents (errors, due dates, the accent itself) are explicit even on
//! a transparent canvas and assume a **dark** terminal background; on a light
//! terminal set `NO_COLOR=1` or `GRC_BG=1`. The test module states and checks
//! that assumption.
//!
//! # Colour depth
//!
//! With `COLORTERM=truecolor` (or `24bit`) the palette is explicit RGB. Without
//! it the app falls back to the sixteen named ANSI colours ([`Depth::Ansi16`]),
//! which every terminal understands; there the selection bar is `DarkGray`
//! carrying bold black text, because no shade between black and dark gray is
//! available. `NO_COLOR=1` drops to the terminal's own colours and marks the
//! selection with reverse video and the gutter glyph instead of a bar.

use crate::tui::state::MessageKind;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Environment variable that switches the UI to ASCII glyphs.
pub const ASCII_ENV: &str = "GRC_ASCII";

/// Standard colour opt-out environment variable.
pub const NO_COLOR_ENV: &str = "NO_COLOR";

/// Opt in to painting the dark canvas behind the interface.
pub const BG_ENV: &str = "GRC_BG";

/// Standard terminal colour-capability variable.
pub const COLORTERM_ENV: &str = "COLORTERM";

/// Panel canvas behind every row when it is painted.
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

/// How much colour the terminal can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// `NO_COLOR` is set: no colour at all.
    None,
    /// No `COLORTERM=truecolor`: the sixteen named ANSI colours.
    Ansi16,
    /// `COLORTERM=truecolor` or `24bit`: explicit RGB.
    TrueColor,
}

/// The colour fields a depth defines.
#[derive(Clone, Copy, Debug)]
struct Palette {
    bg: Color,
    selection: Color,
    selection_unfocused: Color,
    text: Color,
    dim: Color,
    dim_selected: Color,
    accent: Color,
    accent_dim: Color,
    error: Color,
    warning: Color,
    success: Color,
    guide: Color,
    /// A foreground forced on every span of the focused selection bar, for
    /// palettes whose bar colour cannot carry the ordinary foregrounds.
    bar_fg: Option<Color>,
}

impl Palette {
    /// Explicit RGB, tested for contrast against [`BG`] and the selection bar.
    const fn truecolor() -> Self {
        Self {
            bg: BG,
            selection: SELECTION,
            selection_unfocused: SELECTION_UNFOCUSED,
            text: TEXT,
            dim: DIM,
            dim_selected: DIM_SELECTED,
            accent: ACCENT,
            accent_dim: ACCENT_DIM,
            error: ERROR,
            warning: WARNING,
            success: SUCCESS,
            guide: GUIDE,
            bar_fg: None,
        }
    }

    /// Xterm's default sixteen colours.
    ///
    /// The bar is `DarkGray` and carries black text: with only sixteen fixed
    /// shades there is no dark tint that keeps the palette's own foregrounds
    /// readable, and black on dark gray clears 4.5:1. Bold and the gutter
    /// marker keep the row identifiable if a terminal's "dark gray" is dimmer
    /// than xterm's.
    const fn ansi16() -> Self {
        Self {
            bg: Color::Black,
            selection: Color::DarkGray,
            selection_unfocused: Color::Reset,
            text: Color::White,
            dim: Color::Gray,
            dim_selected: Color::White,
            accent: Color::LightCyan,
            accent_dim: Color::Cyan,
            error: Color::LightRed,
            warning: Color::LightYellow,
            success: Color::LightGreen,
            guide: Color::DarkGray,
            bar_fg: Some(Color::Black),
        }
    }

    /// Everything left to the terminal.
    const fn none() -> Self {
        const RESET: Color = Color::Reset;
        Self {
            bg: RESET,
            selection: RESET,
            selection_unfocused: RESET,
            text: RESET,
            dim: RESET,
            dim_selected: RESET,
            accent: RESET,
            accent_dim: RESET,
            error: RESET,
            warning: RESET,
            success: RESET,
            guide: RESET,
            bar_fg: None,
        }
    }
}

/// Box, fold, and cursor glyphs.
///
/// Every glyph is one terminal cell and none of them is in the emoji set: a
/// terminal that substitutes an emoji font draws that line taller than its
/// neighbours, which breaks the one-row-per-line grid the layout assumes.
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
    /// Separator between hint-line entries.
    pub separator: &'static str,
}

impl Glyphs {
    /// Unicode box drawing and symbols.
    pub const fn unicode() -> Self {
        Self {
            checkbox_todo: "□",
            checkbox_done: "✓",
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
            separator: "·",
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
            separator: "|",
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
    /// Colour capability of the terminal.
    pub depth: Depth,
    /// Whether the dark canvas is painted behind the interface.
    pub paint_bg: bool,
    /// Whether ASCII glyphs and borders are in use.
    pub ascii: bool,
    /// Colour actually drawn behind the interface: the palette's canvas when
    /// the canvas is painted, `Reset` (the terminal's own) when it is not.
    pub bg: Color,
    /// The palette's canvas colour, painted or not. Popups always use it: a
    /// popup without a background would show the rows it covers through its own
    /// text.
    pub surface: Color,
    /// Selection bar of the focused panel.
    pub selection: Color,
    /// Selection bar of the unfocused panel, or `Reset` when there is none.
    pub selection_unfocused: Color,
    /// Primary accent.
    pub accent: Color,
    /// Muted accent (breadcrumbs, unfocused marker).
    pub accent_dim: Color,
    /// Primary text on our own backgrounds.
    pub text: Color,
    /// Secondary text on our own backgrounds.
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
    /// Foreground forced on the focused bar, if the palette needs one.
    pub bar_fg: Option<Color>,
    /// Glyph set in use.
    pub glyphs: Glyphs,
}

impl Theme {
    /// Build a theme from the environment.
    ///
    /// `NO_COLOR` wins over everything; `COLORTERM=truecolor`/`24bit` selects
    /// RGB; `GRC_BG` paints the canvas; `GRC_ASCII` swaps every glyph.
    pub fn from_env() -> Self {
        let paint_bg = env_flag(BG_ENV);
        let ascii = env_flag(ASCII_ENV);
        match color_depth() {
            Depth::None => Self::build(Depth::None, paint_bg, ascii),
            Depth::Ansi16 => Self::ansi16(paint_bg, ascii),
            Depth::TrueColor => Self::truecolor(paint_bg, ascii),
        }
    }

    /// Deterministic colourless theme for render tests.
    pub const fn plain() -> Self {
        Self::new(false, false)
    }

    /// Truecolor theme with explicit capabilities.
    ///
    /// `color == false` is the colourless theme; the painted canvas is part of
    /// the colour palette, so it is only ever set in colour mode.
    pub const fn new(color: bool, ascii: bool) -> Self {
        if color {
            Self::build(Depth::TrueColor, true, ascii)
        } else {
            Self::build(Depth::None, false, ascii)
        }
    }

    /// Explicit-RGB theme, with or without the painted canvas.
    pub const fn truecolor(paint_bg: bool, ascii: bool) -> Self {
        Self::build(Depth::TrueColor, paint_bg, ascii)
    }

    /// Sixteen-colour theme, with or without the painted canvas.
    pub const fn ansi16(paint_bg: bool, ascii: bool) -> Self {
        Self::build(Depth::Ansi16, paint_bg, ascii)
    }

    const fn build(depth: Depth, paint_bg: bool, ascii: bool) -> Self {
        let palette = match depth {
            Depth::None => Palette::none(),
            Depth::Ansi16 => Palette::ansi16(),
            Depth::TrueColor => Palette::truecolor(),
        };
        let paint_bg = paint_bg && !matches!(depth, Depth::None);
        Self {
            depth,
            paint_bg,
            ascii,
            bg: if paint_bg { palette.bg } else { Color::Reset },
            surface: palette.bg,
            selection: palette.selection,
            selection_unfocused: palette.selection_unfocused,
            accent: palette.accent,
            accent_dim: palette.accent_dim,
            text: palette.text,
            dim: palette.dim,
            dim_selected: palette.dim_selected,
            error: palette.error,
            warning: palette.warning,
            success: palette.success,
            guide: palette.guide,
            bar_fg: palette.bar_fg,
            glyphs: if ascii {
                Glyphs::ascii()
            } else {
                Glyphs::unicode()
            },
        }
    }

    /// Whether the dark canvas is painted across the whole screen.
    pub const fn paints_canvas(&self) -> bool {
        self.paint_bg
    }

    /// Fill for the whole screen, painted only when [`Theme::paints_canvas`].
    pub fn canvas_style(&self) -> Style {
        Style::default().fg(self.text).bg(self.bg)
    }

    /// Panel interior. Panels are transparent unless the canvas is painted.
    pub fn panel_style(&self) -> Style {
        if self.paint_bg {
            self.canvas_style()
        } else {
            Style::default()
        }
    }

    /// Popup background: always our own canvas colour, so a popup never shows
    /// the panel it covers through its text.
    pub fn popup_style(&self) -> Style {
        if matches!(self.depth, Depth::None) {
            return Style::default();
        }
        Style::default().fg(self.text).bg(self.surface)
    }

    /// Primary text drawn straight on the canvas.
    pub fn canvas_text_style(&self) -> Style {
        if self.paint_bg {
            Style::default().fg(self.text)
        } else {
            Style::default()
        }
    }

    /// Secondary text drawn straight on the canvas.
    ///
    /// On a transparent canvas this keeps the terminal's foreground and dims
    /// it, so it works on both light and dark terminal themes; a fixed grey
    /// cannot.
    pub fn canvas_dim_style(&self) -> Style {
        if self.paint_bg {
            Style::default().fg(self.dim)
        } else {
            Style::default().add_modifier(Modifier::DIM)
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

    /// Whether a row in this state is painted with a selection bar.
    ///
    /// Truecolor paints both the focused and the unfocused bar; the ANSI
    /// palette only has one shade, so its unfocused row is marked with bold and
    /// the gutter instead; the colourless theme uses reverse video.
    pub const fn row_has_bar(&self, selected: bool, focused: bool) -> bool {
        match self.depth {
            Depth::TrueColor => selected,
            Depth::Ansi16 => selected && focused,
            Depth::None => false,
        }
    }

    /// Background of a row, selected or not.
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
    /// foreground each span chose for the bar survives; without a bar it adds
    /// the modifier that marks the row instead.
    pub fn selection_style(&self, focused: bool) -> Style {
        match self.depth {
            Depth::TrueColor => Style::default().bg(self.row_background(true, focused)),
            Depth::Ansi16 if focused => Style::default()
                .bg(self.row_background(true, focused))
                .add_modifier(Modifier::BOLD),
            Depth::Ansi16 => Style::default().add_modifier(Modifier::BOLD),
            Depth::None if focused => {
                Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            }
            Depth::None => Style::default().add_modifier(Modifier::BOLD),
        }
    }

    /// Ordinary body text on the canvas.
    pub fn text_style(&self) -> Style {
        Style::default().fg(self.text)
    }

    /// Secondary, low-emphasis text on the canvas.
    pub fn dim_style(&self) -> Style {
        self.canvas_dim_style()
    }

    /// Progress counters and other quiet metadata.
    pub fn meta_style(&self) -> Style {
        self.canvas_dim_style()
    }

    /// Secondary text inside a popup.
    pub fn popup_dim_style(&self) -> Style {
        Style::default().fg(self.dim)
    }

    /// Coloured mode pill.
    pub fn mode_pill(&self, pill: ModePill) -> Style {
        if matches!(self.depth, Depth::None) {
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
        self.canvas_dim_style()
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
        self.popup_dim_style()
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

    /// Whether this row sits on a painted selection bar.
    fn on_bar(&self) -> bool {
        self.theme.row_has_bar(self.selected, self.focused)
    }

    /// Force the bar foreground when the palette needs one.
    fn on_bar_style(&self, style: Style) -> Style {
        match (self.on_bar(), self.theme.bar_fg) {
            (true, Some(fg)) => style.fg(fg),
            _ => style,
        }
    }

    /// Task or section name.
    pub fn text(&self) -> Style {
        let style = if self.on_bar() {
            Style::default().fg(self.theme.text)
        } else {
            self.theme.canvas_text_style()
        };
        self.on_bar_style(style)
    }

    /// Quiet metadata: progress counters, dates without urgency.
    pub fn dim(&self) -> Style {
        let style = if self.on_bar() {
            Style::default().fg(self.theme.dim_selected)
        } else {
            self.theme.canvas_dim_style()
        };
        self.on_bar_style(style)
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
        self.on_bar_style(Style::default().fg(self.theme.guide))
    }

    /// Fold marker.
    pub fn fold(&self) -> Style {
        self.on_bar_style(Style::default().fg(self.theme.accent))
    }

    /// Section breadcrumb in front of a task from a subsection.
    pub fn breadcrumb(&self) -> Style {
        self.on_bar_style(Style::default().fg(self.theme.accent_dim))
    }

    /// Filter-match emphasis inside row text.
    pub fn matched(&self) -> Style {
        self.on_bar_style(
            Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD),
        )
    }

    /// Inline edit cursor.
    pub fn cursor(&self) -> Style {
        self.on_bar_style(
            Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD),
        )
    }

    /// Text that the user has typed but that cannot be accepted.
    pub fn error(&self) -> Style {
        self.on_bar_style(Style::default().fg(self.theme.error))
    }

    /// Checkbox glyph and style for a task.
    pub fn checkbox(&self, done: bool) -> (&'static str, Style) {
        if done {
            (
                self.theme.glyphs.checkbox_done,
                self.on_bar_style(Style::default().fg(self.theme.success)),
            )
        } else {
            (self.theme.glyphs.checkbox_todo, self.dim())
        }
    }

    /// Due-date colour for a semantic urgency group.
    pub fn due(&self, urgency: DueUrgency) -> Style {
        match urgency {
            DueUrgency::Done | DueUrgency::Later => self.dim(),
            DueUrgency::Overdue => self.on_bar_style(
                Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD),
            ),
            DueUrgency::Today => self.on_bar_style(
                Style::default()
                    .fg(self.theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            DueUrgency::Soon => self.on_bar_style(Style::default().fg(self.theme.accent)),
        }
    }

    /// Style of the gutter marker.
    pub fn marker_style(&self) -> Style {
        self.on_bar_style(
            Style::default()
                .fg(if self.focused {
                    self.theme.accent
                } else {
                    self.theme.accent_dim
                })
                .add_modifier(Modifier::BOLD),
        )
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

/// Whether an environment value opts in (`"0"` and `""` do not).
fn flag_is_set(value: &str) -> bool {
    !value.is_empty() && value != "0"
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| flag_is_set(&value.to_string_lossy()))
}

/// Colour depth from `NO_COLOR` and `COLORTERM`.
fn color_depth() -> Depth {
    if env_flag(NO_COLOR_ENV) {
        return Depth::None;
    }
    depth_from_colorterm(std::env::var(COLORTERM_ENV).ok().as_deref())
}

/// Map `COLORTERM` to a colour depth.
///
/// Only the two values defined by the convention count; anything else (or no
/// value at all) falls back to the sixteen named colours, which are always
/// safe. Terminals that can do better advertise it.
fn depth_from_colorterm(value: Option<&str>) -> Depth {
    match value {
        Some(value)
            if value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit") =>
        {
            Depth::TrueColor
        }
        _ => Depth::Ansi16,
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

    /// An sRGB triple.
    pub(crate) type Rgb = (u8, u8, u8);

    /// Assumed terminal defaults, used when `grc` does not paint its own
    /// background and cannot read the terminal's colours.
    ///
    /// These are the usual defaults of a dark and a light terminal. They are an
    /// assumption, not a measurement: the tests that use them check that the
    /// *interface's* contract holds for these pairs, and the module doc says
    /// what to do when a user's terminal differs.
    pub(crate) const DARK_TERMINAL: (Rgb, Rgb) = ((0, 0, 0), (0xe5, 0xe5, 0xe5));
    /// A light terminal: white background, black foreground.
    pub(crate) const LIGHT_TERMINAL: (Rgb, Rgb) = ((0xff, 0xff, 0xff), (0, 0, 0));

    fn channel(value: u8) -> f64 {
        let value = f64::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    /// sRGB values for the sixteen standard ANSI colours (xterm defaults).
    ///
    /// The truecolor palette is RGB, but the ANSI palette is named and the
    /// contrast tests still have to put a number on it, so the shades are
    /// spelled out here.
    pub(crate) fn sample(color: Color) -> Rgb {
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

    pub(crate) fn luminance_rgb((red, green, blue): Rgb) -> f64 {
        0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
    }

    pub(crate) fn luminance(color: Color) -> f64 {
        luminance_rgb(sample(color))
    }

    /// WCAG 2.1 contrast ratio between two colours.
    pub(crate) fn ratio_rgb(first: Rgb, second: Rgb) -> f64 {
        let (lighter, darker) = {
            let (a, b) = (luminance_rgb(first), luminance_rgb(second));
            if a > b { (a, b) } else { (b, a) }
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    pub(crate) fn ratio(foreground: Color, background: Color) -> f64 {
        ratio_rgb(sample(foreground), sample(background))
    }

    /// A foreground faded `keep` of the way towards the background.
    ///
    /// How much brightness `SGR 2` (faint) removes is up to the terminal; the
    /// tests bound it rather than assume one value.
    pub(crate) fn faint(foreground: Rgb, background: Rgb, keep: f64) -> Rgb {
        let mix = |fg: u8, bg: u8| (f64::from(fg) * keep + f64::from(bg) * (1.0 - keep)) as u8;
        (
            mix(foreground.0, background.0),
            mix(foreground.1, background.1),
            mix(foreground.2, background.2),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::contrast::{
        DARK_TERMINAL, DECOR_MIN, LIGHT_TERMINAL, TEXT_MIN, faint, luminance, ratio as contrast,
        ratio_rgb,
    };
    use super::*;

    /// Assert a row style's foreground against the colour it is actually drawn
    /// on.
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

    /// The colour behind a row: the bar when the palette paints one, otherwise
    /// the canvas.
    fn backdrop(theme: &Theme, selected: bool, focused: bool) -> Color {
        if theme.row_has_bar(selected, focused) {
            theme.row_background(selected, focused)
        } else {
            theme.bg
        }
    }

    /// Themes that paint every surface, so every drawn cell has a colour we
    /// chose and can be measured.
    fn opaque_themes() -> Vec<(String, Theme)> {
        vec![
            ("truecolor".to_string(), Theme::truecolor(true, false)),
            ("ansi16".to_string(), Theme::ansi16(true, false)),
        ]
    }

    #[test]
    fn every_row_style_clears_wcag_contrast() {
        for (palette, theme) in opaque_themes() {
            for (selected, focused) in ROWS {
                let row = RowTheme::new(&theme, selected, focused);
                let background = backdrop(&theme, selected, focused);
                let label = format!("{palette} selected={selected} focused={focused}");
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
    }

    #[test]
    fn every_pill_clears_wcag_contrast() {
        for (palette, theme) in opaque_themes() {
            for pill in [
                ModePill::Normal,
                ModePill::Edit,
                ModePill::Filter,
                ModePill::Help,
                ModePill::Conflict,
            ] {
                let style = theme.mode_pill(pill);
                let background = style.bg.expect("a pill paints its own background");
                assert_contrast(
                    &format!("{palette} pill {pill:?}"),
                    style,
                    background,
                    TEXT_MIN,
                );
            }
        }
    }

    /// The whole point of the transparent canvas: the row colours the app picks
    /// by itself are the terminal's own, so the pair the user configured supplies
    /// the contrast.
    #[test]
    fn a_transparent_canvas_leaves_the_row_text_to_the_terminal() {
        let theme = Theme::truecolor(false, false);
        assert!(!theme.paints_canvas());
        assert_eq!(theme.bg, Color::Reset);
        for (selected, focused) in ROWS {
            if theme.row_has_bar(selected, focused) {
                continue; // checked by the selection-bar test below
            }
            let row = RowTheme::new(&theme, selected, focused);
            let terminal_text = row.text();
            assert_eq!(terminal_text.fg, None, "no fg means the terminal's own");
            assert_eq!(terminal_text.bg, None);
            let secondary = row.dim();
            assert_eq!(secondary.fg, None);
            assert!(
                secondary.add_modifier.contains(Modifier::DIM),
                "secondary text has to stay distinguishable"
            );
        }
        // The assumption the doc states, checked: both terminal defaults the
        // test names clear the threshold for primary text.
        for (background, foreground) in [DARK_TERMINAL, LIGHT_TERMINAL] {
            assert!(ratio_rgb(foreground, background) >= TEXT_MIN);
        }
        // Faint text: terminals keep somewhere around half to two thirds of the
        // foreground's brightness, and both ends stay readable on either
        // terminal.
        for (background, foreground) in [DARK_TERMINAL, LIGHT_TERMINAL] {
            assert!(ratio_rgb(faint(foreground, background, 0.66), background) >= TEXT_MIN);
            assert!(ratio_rgb(faint(foreground, background, 0.5), background) >= DECOR_MIN);
        }
    }

    /// Selecting a row in a transparent canvas still paints a bar, so every
    /// foreground on it is checked against that exact colour.
    #[test]
    fn a_transparent_canvas_still_paints_the_selection_bar() {
        let theme = Theme::truecolor(false, false);
        for (selected, focused) in ROWS {
            if !theme.row_has_bar(selected, focused) {
                continue;
            }
            let row = RowTheme::new(&theme, selected, focused);
            let background = theme.row_background(selected, focused);
            assert_ne!(background, Color::Reset);
            for style in [
                row.text(),
                row.dim(),
                row.done(),
                row.checkbox(false).1,
                row.due(DueUrgency::Overdue),
                row.breadcrumb(),
                row.cursor(),
                row.error(),
            ] {
                assert_contrast("transparent canvas row", style, background, TEXT_MIN);
            }
        }
    }

    /// Coloured accents are explicit even on a transparent canvas, so they
    /// assume a dark terminal background. A user on a light terminal gets the
    /// terminal's own colours via `NO_COLOR=1`, or our tested canvas via
    /// `GRC_BG=1`.
    #[test]
    fn accent_colours_assume_a_dark_terminal() {
        let theme = Theme::truecolor(false, false);
        let (dark_bg, _) = DARK_TERMINAL;
        for (name, color, minimum) in [
            ("text", theme.text, TEXT_MIN),
            ("dim", theme.dim, TEXT_MIN),
            ("accent", theme.accent, TEXT_MIN),
            ("accent dim", theme.accent_dim, TEXT_MIN),
            ("error", theme.error, TEXT_MIN),
            ("warning", theme.warning, TEXT_MIN),
            ("success", theme.success, TEXT_MIN),
            ("guide", theme.guide, DECOR_MIN),
        ] {
            let ratio = ratio_rgb(contrast::sample(color), dark_bg);
            assert!(
                ratio >= minimum,
                "{name}: {color:?} on a dark terminal is {ratio:.2}:1, needs {minimum}:1"
            );
        }
        // Documented limitation, pinned so nobody assumes a light terminal is
        // covered: these accents do not clear the threshold on white.
        let (light_bg, _) = LIGHT_TERMINAL;
        for color in [theme.error, theme.warning, theme.success] {
            assert!(
                ratio_rgb(contrast::sample(color), light_bg) < TEXT_MIN,
                "{color:?} unexpectedly readable on a light terminal; the doc needs updating"
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
        let theme = Theme::truecolor(true, false);
        assert_ne!(theme.selection, theme.selection_unfocused);
        let focused = luminance(theme.selection);
        let unfocused = luminance(theme.selection_unfocused);
        let canvas = luminance(theme.bg);
        assert!(canvas < unfocused && unfocused < focused);
    }

    /// The dark canvas is opt-in: with no `GRC_BG` the terminal's background
    /// shows through.
    #[test]
    fn the_canvas_is_only_painted_on_request() {
        assert!(Theme::truecolor(true, false).paints_canvas());
        assert_eq!(Theme::truecolor(true, false).bg, BG);
        let transparent = Theme::truecolor(false, false);
        assert!(!transparent.paints_canvas());
        assert_eq!(transparent.bg, Color::Reset);
        assert_eq!(transparent.panel_style(), Style::default());
        // Popups are painted either way, from the palette's own canvas colour.
        assert_eq!(transparent.popup_style().bg, Some(BG));
        assert_eq!(transparent.surface, BG);
    }

    /// With `NO_COLOR` there is no palette to paint, so `GRC_BG` is ignored
    /// rather than filling the screen with a colour the user opted out of.
    #[test]
    fn no_colour_ignores_the_canvas_request() {
        let theme = Theme::build(Depth::None, true, false);
        assert!(!theme.paints_canvas());
        assert_eq!(theme.bg, Color::Reset);
        assert_eq!(theme.panel_style(), Style::default());
        assert_eq!(theme.popup_style(), Style::default());
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
        assert!(!plain.row_has_bar(true, true));

        // Sixteen colours: one bar shade, so the unfocused row is marked with
        // bold and the gutter instead of a second bar.
        let ansi = Theme::ansi16(true, false);
        assert_eq!(ansi.selection, Color::DarkGray);
        assert_eq!(ansi.selection_style(true).bg, Some(Color::DarkGray));
        assert!(ansi.row_has_bar(true, true));
        assert!(!ansi.row_has_bar(true, false));
        assert!(ansi.selection_style(false).bg.is_none());

        assert_eq!(Theme::new(false, true).glyphs.selection_marker, "|");
    }

    /// The ANSI bar cannot use the palette's own foregrounds, so it carries
    /// black text; that is the pairing the contrast test measures.
    #[test]
    fn the_ansi_bar_carries_black_text() {
        let ansi = Theme::ansi16(true, false);
        assert_eq!(ansi.bar_fg, Some(Color::Black));
        let row = RowTheme::new(&ansi, true, true);
        for style in [row.text(), row.dim(), row.due(DueUrgency::Overdue)] {
            assert_eq!(style.fg, Some(Color::Black));
            assert!(contrast(Color::Black, Color::DarkGray) >= TEXT_MIN);
        }
        // The truecolor palette needs no override.
        assert_eq!(Theme::truecolor(true, false).bar_fg, None);
    }

    #[test]
    fn colour_depth_follows_colorterm() {
        for value in ["truecolor", "24bit", "TRUECOLOR", "24BIT"] {
            assert_eq!(
                depth_from_colorterm(Some(value)),
                Depth::TrueColor,
                "{value}"
            );
        }
        for value in ["16color", "256color", "", "yes"] {
            assert_eq!(depth_from_colorterm(Some(value)), Depth::Ansi16, "{value}");
        }
        assert_eq!(depth_from_colorterm(None), Depth::Ansi16);
    }

    #[test]
    fn environment_flags_only_opt_in_explicitly() {
        assert!(flag_is_set("1"));
        assert!(flag_is_set("yes"));
        assert!(!flag_is_set(""));
        assert!(!flag_is_set("0"));
    }

    #[test]
    fn the_marker_fills_exactly_one_gutter_cell() {
        use crate::tui::widgets::{GUTTER, display_width};
        for glyphs in [Glyphs::unicode(), Glyphs::ascii()] {
            assert_eq!(display_width(glyphs.selection_marker), GUTTER);
        }
    }

    fn every_glyph(glyphs: Glyphs) -> [(&'static str, &'static str); 14] {
        [
            ("checkbox_todo", glyphs.checkbox_todo),
            ("checkbox_done", glyphs.checkbox_done),
            ("fold_open", glyphs.fold_open),
            ("fold_closed", glyphs.fold_closed),
            ("tree_guide", glyphs.tree_guide),
            ("cursor", glyphs.cursor),
            ("breadcrumb_sep", glyphs.breadcrumb_sep),
            ("invalid_due", glyphs.invalid_due),
            ("preview", glyphs.preview),
            ("ellipsis", glyphs.ellipsis),
            ("selection_marker", glyphs.selection_marker),
            ("scroll_track", glyphs.scroll_track),
            ("scroll_thumb", glyphs.scroll_thumb),
            ("separator", glyphs.separator),
        ]
    }

    /// Code points a terminal may draw with an emoji font, making the row
    /// taller than its neighbours.
    const EMOJI_CAPABLE: [char; 10] = [
        '\u{2610}',  // ☐ BALLOT BOX (what the checkbox used to be)
        '\u{2611}',  // ☑ BALLOT BOX WITH CHECK
        '\u{2705}',  // ✅
        '\u{2714}',  // ✔
        '\u{274c}',  // ❌
        '\u{2b1b}',  // ⬛
        '\u{2b1c}',  // ⬜
        '\u{1f5f8}', // 🗸
        '\u{1f5f9}', // 🗹
        '\u{fe0f}',  // emoji variation selector
    ];

    #[test]
    fn no_glyph_can_be_drawn_as_emoji() {
        for glyphs in [Glyphs::unicode(), Glyphs::ascii()] {
            for (name, glyph) in every_glyph(glyphs) {
                for character in glyph.chars() {
                    assert!(
                        !EMOJI_CAPABLE.contains(&character),
                        "{name} ({glyph:?}) can be drawn as emoji, which makes its row taller"
                    );
                }
            }
        }
    }

    /// Pinned on purpose: the layout arithmetic assumes one cell per glyph, so
    /// a new glyph has to be added here (and checked by the test above).
    #[test]
    fn the_ascii_theme_swaps_the_borders_too() {
        use crate::tui::widgets::border_set;
        let unicode = border_set(&Theme::truecolor(true, false));
        assert_eq!(unicode.top_left, "╭");
        let ascii = border_set(&Theme::truecolor(true, true));
        let glyphs = [
            ascii.top_left,
            ascii.top_right,
            ascii.bottom_left,
            ascii.bottom_right,
            ascii.vertical_left,
            ascii.vertical_right,
            ascii.horizontal_top,
            ascii.horizontal_bottom,
        ];
        for glyph in glyphs {
            assert!(glyph.is_ascii(), "{glyph:?} is not ASCII");
        }
        assert_eq!(
            (ascii.top_left, ascii.vertical_left, ascii.horizontal_top),
            ("+", "|", "-")
        );
        // The separators follow the same switch.
        assert_eq!(Glyphs::ascii().separator, "|");
        assert_eq!(Glyphs::unicode().separator, "·");
    }

    #[test]
    fn the_glyph_set_is_exactly_this() {
        assert_eq!(
            every_glyph(Glyphs::unicode()).map(|(_, glyph)| glyph),
            [
                "□", "✓", "▾", "▸", "│", "│", "›", "!", "→", "…", "▌", "│", "█", "·"
            ]
        );
        assert_eq!(
            every_glyph(Glyphs::ascii()).map(|(_, glyph)| glyph),
            [
                "[ ]", "[x]", "v", ">", "|", "|", ">", "!", "->", "...", "|", "|", "#", "|"
            ]
        );
    }

    #[test]
    fn the_ascii_glyph_set_is_pure_ascii() {
        for (name, glyph) in every_glyph(Glyphs::ascii()) {
            assert!(glyph.is_ascii(), "{name} ({glyph:?}) is not ASCII");
        }
    }

    #[test]
    fn single_cell_glyphs_are_one_cell_wide() {
        use crate::tui::widgets::display_width;
        for glyphs in [Glyphs::unicode(), Glyphs::ascii()] {
            for (name, glyph) in every_glyph(glyphs) {
                if glyph.chars().count() > 1 && glyph != "[ ]" && glyph != "[x]" {
                    continue; // multi-character markers are measured by their users
                }
                if glyph == "[ ]" || glyph == "[x]" {
                    continue;
                }
                assert_eq!(display_width(glyph), 1, "{name} is not a single cell");
            }
        }
    }

    #[test]
    fn a_selected_row_never_reuses_the_canvas_secondary_colour() {
        let theme = Theme::truecolor(true, false);
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
