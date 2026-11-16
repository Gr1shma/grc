//! Top-level layout and chrome.

use crate::task::TodoList;
use crate::tui::state::{AppState, Mode};
use crate::tui::theme::Theme;
use crate::tui::widgets::{hint, status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use std::path::Path;

pub(crate) mod help;
mod left;
mod right;

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
mod snapshots {
    use super::*;
    use crate::parser::parse_str;
    use crate::tui::state::{AppState, DueInput, Focus, Mode, TextField};
    use crate::tui::task_ref::TaskRef;
    use crate::tui::tree::build_visible_tree_items;
    use chrono::NaiveDate;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
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

    fn render(width: u16, height: u16, todo: &TodoList, app: &AppState) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw_ui(frame, todo, app, &Theme::plain(), Path::new("/tmp/todo.md")))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
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
        assert!(HELP.contains("Edit"));
        // Dates and Misc are below the fold; the help module tests all groups.
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
}
