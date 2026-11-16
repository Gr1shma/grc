//! Top-level layout and chrome.

use crate::task::TodoList;
use crate::tui::state::{AppState, Focus, Mode};
use crate::tui::theme::Theme;
use crate::tui::widgets::{hint, status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::Block;
use std::path::Path;

pub(crate) mod help;
mod left;
mod right;

/// Which of the two panels a piece of logic refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    /// Section tree on the left.
    Left,
    /// Task list on the right.
    Right,
}

impl Panel {
    /// Whether this panel currently shows the focused chrome and the strong
    /// selection bar.
    ///
    /// Shared with the tests so the strength of a selection bar can never
    /// disagree with what was drawn. Editing a task keeps the task panel
    /// focused; renaming a section keeps the tree focused.
    pub fn is_focused(self, app: &AppState) -> bool {
        match self {
            Panel::Left => {
                app.focus == Focus::Left
                    && !matches!(
                        app.mode,
                        Mode::Help
                            | Mode::Conflict
                            | Mode::Filter
                            | Mode::InputTask(_)
                            | Mode::InputDue(_)
                    )
            }
            Panel::Right => {
                matches!(app.mode, Mode::InputTask(_) | Mode::InputDue(_))
                    || (app.focus == Focus::Right && matches!(app.mode, Mode::Normal))
            }
        }
    }
}

/// Horizontal split between the section tree and the task panel.
///
/// Shared with mouse hit-testing so a click can never land in a different panel
/// from the one that was drawn under the cursor; ratatui's percentage rounding
/// is not reproducible with a plain `width * percent / 100`.
pub fn panel_split(area: Rect) -> [Rect; 2] {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(crate::tui::LEFT_PANEL_PERCENT),
            Constraint::Percentage(100 - crate::tui::LEFT_PANEL_PERCENT),
        ])
        .split(area);
    [panels[0], panels[1]]
}

/// Draw the whole interface from immutable state.
pub fn draw_ui(f: &mut Frame, todo_list: &TodoList, app: &AppState, theme: &Theme, path: &Path) {
    let area = f.area();
    // The canvas is the terminal's own background unless `GRC_BG=1` asked for
    // ours; painting it always would break transparency.
    if theme.paints_canvas() {
        f.render_widget(Block::default().style(theme.canvas_style()), area);
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let [left_area, right_area] = panel_split(chunks[0]);
    left::draw_left(f, todo_list, app, theme, left_area);
    right::draw_right(f, todo_list, app, theme, right_area);
    hint::draw(f, app, chunks[1], theme);
    status::draw(f, todo_list, app, path, chunks[2], theme);

    if matches!(app.mode, Mode::Help) {
        help::draw_help_overlay(f, app, theme, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_str;
    use crate::tui::state::{AppState, DueInput, Focus, Mode, TaskInput, TextField};
    use crate::tui::task_ref::TaskRef;
    use crate::tui::theme::contrast::{DECOR_MIN, TEXT_MIN, ratio};
    use crate::tui::tree::build_visible_tree_items;
    use chrono::NaiveDate;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::{Color, Modifier};
    use std::path::Path;

    const TODAY: NaiveDate = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();

    fn prepare(app: &mut AppState, todo: &TodoList) {
        app.tree_items = build_visible_tree_items(
            todo,
            &app.mode,
            &app.filter_lower,
            &app.folded,
            app.hide_done,
        );
    }

    fn cells(width: u16, height: u16, todo: &TodoList, app: &AppState, theme: &Theme) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw_ui(frame, todo, app, theme, Path::new("/tmp/todo.md")))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn render_with(
        theme: &Theme,
        width: u16,
        height: u16,
        todo: &TodoList,
        app: &AppState,
    ) -> String {
        let buffer = cells(width, height, todo, app, theme);
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buffer[(x, y)].symbol());
            }
            if y + 1 < height {
                out.push('\n');
            }
        }
        out
    }

    fn render(width: u16, height: u16, todo: &TodoList, app: &AppState) -> String {
        render_with(&Theme::plain(), width, height, todo, app)
    }

    fn normal_state() -> (TodoList, AppState) {
        let todo = parse_str(
            "# Work\n- [ ] Ship release due:today\n- [x] Write docs\n- [ ] Plan next week due:2027-03-01\n## Urgent\n- [ ] Fix bug due:tomorrow\n- [ ] Call back\n# Home\n- [ ] Groceries\n",
        );
        let mut app = AppState::new_with_today(2, TODAY);
        app.focus = Focus::Right;
        prepare(&mut app, &todo);
        (todo, app)
    }

    /// Compare a rendered frame with its checked-in snapshot.
    ///
    /// Run `GRC_BLESS=1 cargo test` to rewrite the snapshots after an
    /// intentional rendering change; the next compile picks up the new files.
    fn assert_snapshot(name: &str, expected: &str, actual: String) {
        if std::env::var_os("GRC_BLESS").is_some() {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/tui/render/snapshots")
                .join(format!("{name}.txt"));
            std::fs::write(path, &actual).expect("write snapshot");
            return;
        }
        assert_eq!(actual, expected, "snapshot `{name}` changed");
    }

    const NORMAL: &str = include_str!("snapshots/normal.txt");
    const EDIT: &str = include_str!("snapshots/edit.txt");
    const EDIT_INVALID_DUE: &str = include_str!("snapshots/edit_invalid_due.txt");
    const FILTER: &str = include_str!("snapshots/filter.txt");
    const HELP: &str = include_str!("snapshots/help.txt");
    const EMPTY: &str = include_str!("snapshots/empty.txt");
    const NARROW: &str = include_str!("snapshots/narrow.txt");
    const CONFLICT: &str = include_str!("snapshots/conflict.txt");

    #[test]
    fn normal_snapshot() {
        let (todo, app) = normal_state();
        assert_snapshot("normal", NORMAL, render(72, 14, &todo, &app));
    }

    #[test]
    fn edit_snapshot_shows_cursor_and_inline_due_preview() {
        let (todo, mut app) = normal_state();
        app.mode = Mode::InputTask(crate::tui::state::TaskInput {
            editing_idx: Some(0),
            insert_idx: None,
            above: false,
            node: vec![0],
            field: TextField::new("Ship release due:next fri"),
        });
        prepare(&mut app, &todo);
        assert_snapshot("edit", EDIT, render(72, 14, &todo, &app));
        assert!(EDIT.contains("│  → fri"));
    }

    #[test]
    fn edit_snapshot_shows_invalid_due_warning() {
        let (todo, mut app) = normal_state();
        app.mode = Mode::InputDue(DueInput {
            target: TaskRef {
                node: vec![0],
                task_idx: 0,
                sub_name: None,
            },
            field: TextField::new("nonsense"),
        });
        prepare(&mut app, &todo);
        assert_snapshot(
            "edit_invalid_due",
            EDIT_INVALID_DUE,
            render(72, 14, &todo, &app),
        );
        assert!(EDIT_INVALID_DUE.contains("invalid — old date kept"));
    }

    #[test]
    fn filter_snapshot_shows_prompt_and_match_count() {
        let (todo, mut app) = normal_state();
        app.mode = Mode::Filter;
        app.set_filter("bug".into());
        prepare(&mut app, &todo);
        assert_snapshot("filter", FILTER, render(72, 14, &todo, &app));
        assert!(FILTER.contains("/bug  1 match"));
    }

    #[test]
    fn help_snapshot_is_grouped_and_scrollable() {
        let (todo, mut app) = normal_state();
        app.mode = Mode::Help;
        app.viewport_height = 20;
        prepare(&mut app, &todo);
        assert_snapshot("help", HELP, render(80, 20, &todo, &app));
        assert!(HELP.contains("Navigate"));
        assert!(HELP.contains("Sections"));
        // Tasks, Dates and Misc are below the fold; the help module tests them.
    }

    #[test]
    fn empty_snapshot_shows_a_hint() {
        let empty = parse_str("# Empty\n");
        let mut app = AppState::new_with_today(1, TODAY);
        app.focus = Focus::Right;
        prepare(&mut app, &empty);
        assert_snapshot("empty", EMPTY, render(56, 10, &empty, &app));
        assert!(EMPTY.contains("no tasks — press o to add one"));
    }

    #[test]
    fn narrow_snapshot_degrades_without_overlap() {
        let (todo, app) = normal_state();
        assert_snapshot("narrow", NARROW, render(40, 10, &todo, &app));
        assert!(NARROW.contains("today"));
    }

    #[test]
    fn conflict_snapshot_offers_reload_and_overwrite() {
        let (todo, mut app) = normal_state();
        app.mode = Mode::Conflict;
        app.set_error("file changed on disk — r reload / o overwrite");
        prepare(&mut app, &todo);
        assert_snapshot("conflict", CONFLICT, render(72, 14, &todo, &app));
        assert!(CONFLICT.contains("CONFLICT"));
        assert!(CONFLICT.contains("r reload"));
    }

    #[test]
    fn long_task_lists_draw_a_scrollbar() {
        let mut content = String::from("# main\n");
        for index in 0..30 {
            content.push_str(&format!("- [ ] task {index}\n"));
        }
        let todo = parse_str(&content);
        let mut app = AppState::new_with_today(1, TODAY);
        app.focus = Focus::Right;
        prepare(&mut app, &todo);
        let rendered = render(60, 12, &todo, &app);
        assert!(
            rendered.contains('█'),
            "expected a scrollbar thumb:\n{rendered}"
        );
    }

    #[test]
    fn tiny_sizes_do_not_panic() {
        let (todo, mut app) = normal_state();
        for size in [(0, 0), (1, 1), (2, 2), (3, 3), (5, 2), (8, 4), (40, 10)] {
            let _ = render(size.0, size.1, &todo, &app);
            app.mode = Mode::Help;
            let _ = render(size.0, size.1, &todo, &app);
            app.mode = Mode::Normal;
        }
    }

    #[test]
    fn injected_today_controls_due_labels() {
        let (todo, mut app) = normal_state();
        let rendered = render(72, 14, &todo, &app);
        assert!(rendered.contains("today"));
        assert!(rendered.contains("tmr"));

        app.today = TODAY.succ_opt().unwrap();
        let tomorrow = render(72, 14, &todo, &app);
        assert_ne!(rendered, tomorrow);
        assert!(tomorrow.contains("yday"));
    }

    #[test]
    fn long_rows_truncate_with_an_ellipsis_and_keep_the_due_column() {
        let todo = parse_str(
            "# Work\n- [ ] Ship a release that needs a much longer name due:today\n## Urgent\n- [ ] A subsection task with a very long name due:tomorrow\n",
        );
        for width in [40, 20] {
            let mut app = AppState::new_with_today(2, TODAY);
            app.focus = Focus::Right;
            prepare(&mut app, &todo);
            let buffer = cells(width, 14, &todo, &app, &Theme::plain());
            let [_, right] = panel_split(Rect::new(0, 0, width, 12));
            let rendered = render(width, 14, &todo, &app);
            assert!(
                rendered.contains('…'),
                "width {width}: expected a truncated row:\n{rendered}"
            );
            // Every drawn row keeps its right border, and the due label stays
            // whole: text is cut, the due column is not overwritten.
            for row in 1..=4 {
                let last = right.x + right.width - 1;
                assert_eq!(
                    buffer[(last, row)].symbol(),
                    "│",
                    "width {width} row {row}: panel border overwritten:\n{rendered}"
                );
            }
            assert!(
                rendered.contains("today") || rendered.contains("tmr"),
                "width {width}: due column was squeezed out:\n{rendered}"
            );
        }
    }

    /// Draw the row `index` rows below a panel's top border.
    fn drawn_row(buffer: &Buffer, panel: Rect, index: usize) -> Vec<ratatui::buffer::Cell> {
        let y = panel.y + 1 + u16::try_from(index).unwrap();
        (panel.x + 1..panel.x + panel.width - 1)
            .map(|x| buffer[(x, y)].clone())
            .collect()
    }

    /// Glyphs that decorate a row instead of carrying text.
    fn is_decoration(theme: &Theme, symbol: &str) -> bool {
        let glyphs = &theme.glyphs;
        [
            glyphs.tree_guide,
            glyphs.fold_open,
            glyphs.fold_closed,
            glyphs.selection_marker,
            glyphs.scroll_track,
            glyphs.scroll_thumb,
        ]
        .contains(&symbol)
    }

    /// The property the old gray bar broke: every cell drawn on a row must be
    /// legible against the background that cell actually landed on.
    fn assert_row_is_legible(cells: &[ratatui::buffer::Cell], theme: &Theme, what: &str) {
        for cell in cells {
            assert_ne!(
                cell.fg,
                Color::DarkGray,
                "{what}: {:?} kept the old gray foreground",
                cell.symbol()
            );
            if cell.symbol().trim().is_empty() {
                continue;
            }
            let minimum = if is_decoration(theme, cell.symbol()) {
                DECOR_MIN
            } else {
                TEXT_MIN
            };
            let ratio = ratio(cell.fg, cell.bg);
            assert!(
                ratio >= minimum,
                "{what}: {:?} {:?} on {:?} is {ratio:.2}:1, needs {minimum}:1",
                cell.symbol(),
                cell.fg,
                cell.bg
            );
        }
    }

    fn selected_row_is_legible(
        app: &AppState,
        todo: &TodoList,
        panel: Panel,
        index: usize,
        what: &str,
    ) {
        let theme = Theme::new(true, false);
        let buffer = cells(72, 14, todo, app, &theme);
        let area = panel_split(Rect::new(0, 0, 72, 12))[panel_index(panel)];
        let cells = drawn_row(&buffer, area, index);
        let expected = if panel.is_focused(app) {
            theme.selection
        } else {
            theme.selection_unfocused
        };
        for cell in &cells {
            assert_eq!(cell.bg, expected, "{what}: cell not on the selection bar");
            // The colours chosen for the panel canvas are not all readable on
            // the bar, so a selected row must never fall back to them.
            assert_ne!(
                cell.fg,
                theme.dim,
                "{what}: {:?} used the canvas secondary colour on the bar",
                cell.symbol()
            );
            assert_ne!(
                cell.fg,
                theme.bg,
                "{what}: {:?} is invisible on the bar",
                cell.symbol()
            );
        }
        assert_row_is_legible(&cells, &theme, what);
    }

    fn panel_index(panel: Panel) -> usize {
        match panel {
            Panel::Left => 0,
            Panel::Right => 1,
        }
    }

    /// A document with a done task, an overdue task, and a filter match.
    fn selection_state() -> (TodoList, AppState) {
        let todo = parse_str(
            "# Work\n- [ ] Ship release due:today\n- [x] Write docs\n- [ ] Pay rent due:2026-11-09\n## Urgent\n- [ ] Fix bug\n",
        );
        let mut app = AppState::new_with_today(2, TODAY);
        app.focus = Focus::Right;
        prepare(&mut app, &todo);
        (todo, app)
    }

    #[test]
    fn the_selected_task_row_sits_on_the_selection_bar() {
        let (todo, mut app) = selection_state();
        // Every kind of task text a bar can carry: plain, done, overdue.
        for (index, what) in [(0, "todo"), (1, "done task"), (2, "overdue task")] {
            app.right_state.select(Some(index));
            selected_row_is_legible(&app, &todo, Panel::Right, index, what);
        }
    }

    #[test]
    fn a_subsections_breadcrumb_stays_legible_on_the_bar() {
        let (todo, mut app) = selection_state();
        app.left_state.select(Some(1));
        prepare(&mut app, &todo);
        app.right_state.select(Some(0));
        assert!(
            render(72, 14, &todo, &app).contains('›'),
            "expected a breadcrumb row"
        );
        selected_row_is_legible(&app, &todo, Panel::Right, 0, "breadcrumb");
    }

    #[test]
    fn the_selected_section_row_is_legible_including_its_guides() {
        let (todo, mut app) = selection_state();
        app.focus = Focus::Left;
        // Row 1 is the nested section, so its row carries a tree guide.
        app.left_state.select(Some(1));
        selected_row_is_legible(&app, &todo, Panel::Left, 1, "nested section");
        app.left_state.select(Some(0));
        selected_row_is_legible(&app, &todo, Panel::Left, 0, "top-level section");
    }

    #[test]
    fn a_filter_match_on_the_selected_row_is_still_legible() {
        let (todo, mut app) = selection_state();
        app.set_filter("ship".into());
        prepare(&mut app, &todo);
        app.right_state.select(Some(0));
        selected_row_is_legible(&app, &todo, Panel::Right, 0, "filter match");
    }

    #[test]
    fn the_row_being_edited_is_legible_on_the_bar() {
        let (todo, mut app) = selection_state();
        app.mode = Mode::InputTask(TaskInput {
            editing_idx: Some(0),
            insert_idx: None,
            above: false,
            node: vec![0],
            field: TextField::new("Ship release due:next fri"),
        });
        prepare(&mut app, &todo);
        app.right_state.select(Some(0));
        selected_row_is_legible(&app, &todo, Panel::Right, 0, "edited row");
    }

    #[test]
    fn an_unfocused_panels_selection_is_weaker_but_still_legible() {
        let (todo, mut app) = selection_state();
        app.focus = Focus::Left;
        app.right_state.select(Some(2));
        assert!(!Panel::Right.is_focused(&app));
        selected_row_is_legible(&app, &todo, Panel::Right, 2, "unfocused panel");

        let theme = Theme::new(true, false);
        let canvas = crate::tui::theme::contrast::luminance(theme.bg);
        let unfocused = crate::tui::theme::contrast::luminance(theme.selection_unfocused);
        let focused = crate::tui::theme::contrast::luminance(theme.selection);
        assert!(canvas < unfocused && unfocused < focused);
    }

    #[test]
    fn the_marker_sits_in_the_gutter_of_exactly_the_selected_row() {
        let (todo, mut app) = selection_state();
        app.right_state.select(Some(1));
        let theme = Theme::new(true, false);
        let buffer = cells(72, 14, &todo, &app, &theme);
        let [left, right] = panel_split(Rect::new(0, 0, 72, 12));
        let marker = theme.glyphs.selection_marker;
        // The task panel selects row 1 (drawn second); the tree keeps row 0.
        assert_eq!(buffer[(right.x + 1, 2)].symbol(), marker);
        assert_eq!(buffer[(right.x + 1, 1)].symbol(), " ");
        assert_eq!(buffer[(right.x + 1, 3)].symbol(), " ");
        assert_eq!(buffer[(left.x + 1, 1)].symbol(), marker);
        assert_eq!(buffer[(left.x + 1, 2)].symbol(), " ");
    }

    #[test]
    fn the_no_colour_theme_draws_no_backgrounds_at_all() {
        let (todo, app) = normal_state();
        let theme = Theme::new(false, false);
        let buffer = cells(72, 14, &todo, &app, &theme);
        for y in 0..14 {
            for x in 0..72 {
                assert_eq!(
                    buffer[(x, y)].bg,
                    Color::Reset,
                    "({x},{y}) kept a background without colour support"
                );
            }
        }
        assert_eq!(theme.selection_style(true).bg, None);
    }

    #[test]
    fn a_transparent_theme_paints_only_the_selection_bars() {
        let (todo, mut app) = selection_state();
        app.right_state.select(Some(1));
        // No `GRC_BG`: the terminal's background shows through everywhere but
        // the two selection bars.
        let theme = Theme::truecolor(false, false);
        let buffer = cells(72, 14, &todo, &app, &theme);
        let [left, right] = panel_split(Rect::new(0, 0, 72, 12));
        // The bars cover the lists' inner width; the panel borders are chrome.
        let inside = |panel: Rect| (panel.x + 1)..(panel.x + panel.width - 1);
        for x in inside(right) {
            assert_eq!(buffer[(x, 2)].bg, theme.selection, "({x},2) lost the bar");
        }
        for x in inside(left) {
            assert_eq!(
                buffer[(x, 1)].bg,
                theme.selection_unfocused,
                "({x},1) lost the unfocused bar"
            );
        }
        // Only the panels: the status bar paints its own mode pill, which is
        // checked by its own test.
        for y in 0..12 {
            for x in 0..72 {
                let on_a_bar =
                    (y == 2 && inside(right).contains(&x)) || (y == 1 && inside(left).contains(&x));
                assert!(
                    on_a_bar || buffer[(x, y)].bg == Color::Reset,
                    "({x},{y}) painted a background the user did not ask for"
                );
            }
        }
        // Unselected text is left to the terminal, so a light terminal theme
        // supplies the contrast instead of fighting ours.
        assert_eq!(buffer[(right.x + 2, 3)].fg, Color::Reset);
    }

    #[test]
    fn a_popup_paints_its_own_background_on_a_transparent_canvas() {
        let (todo, mut app) = normal_state();
        app.mode = Mode::Help;
        app.viewport_height = 20;
        prepare(&mut app, &todo);
        let theme = Theme::truecolor(false, false);
        let buffer = cells(80, 20, &todo, &app, &theme);
        let popup = crate::tui::help_popup(Rect::new(0, 0, 80, 20));
        for y in popup.y..popup.y + popup.height {
            for x in popup.x..popup.x + popup.width {
                assert_eq!(
                    buffer[(x, y)].bg,
                    theme.surface,
                    "({x},{y}) let the panel show through the popup"
                );
            }
        }
    }

    const NO_COLOUR: &str = include_str!("snapshots/no_colour.txt");

    #[test]
    fn no_colour_snapshot_keeps_the_selection_visible() {
        let (todo, app) = normal_state();
        let theme = Theme::new(false, false);
        let frame = render_with(&theme, 72, 14, &todo, &app);
        assert_snapshot("no_colour", NO_COLOUR, frame.clone());
        assert!(frame.contains('▌'), "the gutter bar marks the selection");
        assert!(frame.contains('□') && frame.contains('✓'));
        // Colour is gone, so the bar has to be carried by the attribute alone.
        let buffer = cells(72, 14, &todo, &app, &theme);
        let [_, right] = panel_split(Rect::new(0, 0, 72, 12));
        let cell = &buffer[(right.x + 2, 1)];
        assert!(cell.modifier.contains(Modifier::BOLD | Modifier::REVERSED));
    }

    const ASCII: &str = include_str!("snapshots/ascii.txt");

    #[test]
    fn ascii_snapshot_uses_only_ascii_glyphs() {
        let (todo, app) = normal_state();
        let theme = Theme::new(true, true);
        let frame = render_with(&theme, 72, 14, &todo, &app);
        assert_snapshot("ascii", ASCII, frame.clone());
        // `GRC_ASCII` covers the data glyphs — checkboxes, folds, the selection
        // marker, the ellipsis, the scrollbar — not the box-drawing chrome,
        // which every terminal that can run a TUI draws.
        assert!(frame.contains("[ ]") && frame.contains("[x]"));
        assert!(frame.contains("|v Work"), "the fold and gutter markers");
        for glyph in ['□', '✓', '▾', '▌', '…', '›', '█'] {
            assert!(!frame.contains(glyph), "{glyph} survived GRC_ASCII");
        }
    }

    #[test]
    fn no_colour_and_ascii_can_be_combined() {
        let (todo, app) = normal_state();
        let frame = render_with(&Theme::new(false, true), 72, 14, &todo, &app);
        assert!(frame.contains("|[ ] Ship release"));
        assert!(!frame.contains('▌'), "the ASCII bar still marks the row");
    }
}
