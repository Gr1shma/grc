//! Terminal lifecycle and the main event loop.
//!
//! `run_tui` owns raw mode, the alternate screen, and mouse capture. A guard
//! restores all three on normal exit, on error, and during panic unwinding.
//! `run_engine` owns the document and UI state but no terminal setup.

pub mod actions;
pub mod engine;
pub mod input;
pub mod render;
pub mod state;
pub mod task_ref;
pub mod theme;
pub mod tree;
pub mod widgets;

use crate::task::TodoList;
use crate::tui::engine::Engine;
use crate::tui::state::{AppState, Focus, Mode};
use crate::tui::task_ref::get_task_refs_filtered;
use crate::tui::tree::{clamp_right_selection, selected_node};
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use std::io::{self, Stdout};
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Width of the section panel as a percentage of terminal width.
pub const LEFT_PANEL_PERCENT: u16 = 27;

/// Help popup width percentage.
pub const HELP_POPUP_WIDTH_PERCENT: u16 = 70;

/// Help popup height in rows.
pub const HELP_POPUP_HEIGHT: u16 = 36;

/// Run the TUI until the user quits.
pub fn run_tui(path: &Path) -> Result<()> {
    let mut session = TerminalSession::enter()?;
    let previous_hook = Arc::new(panic::take_hook());
    {
        let previous_hook = Arc::clone(&previous_hook);
        panic::set_hook(Box::new(move |info| {
            // The default hook prints while the alternate screen is still active,
            // which would hide the message. Restore first, then delegate.
            restore_terminal(None);
            previous_hook(info);
        }));
    }

    let result = panic::catch_unwind(AssertUnwindSafe(|| run_engine(&mut session.terminal, path)));
    drop(session);
    let _ = panic::take_hook();
    panic::set_hook(Box::new(move |info| previous_hook(info)));

    match result {
        Ok(result) => result,
        Err(panic) => Err(anyhow::anyhow!(
            "TUI panicked: {}",
            panic_message(panic.as_ref())
        )),
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Terminal state that must be restored on every exit path.
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;

        let terminal = (|| -> Result<Terminal<CrosstermBackend<Stdout>>> {
            let mut out = io::stdout();
            execute!(out, EnterAlternateScreen).context("enter alternate screen")?;
            execute!(out, event::EnableMouseCapture).context("enable mouse capture")?;
            Terminal::new(CrosstermBackend::new(out)).context("create terminal")
        })();

        match terminal {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                restore_terminal(None);
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        restore_terminal(Some(&mut self.terminal));
    }
}

fn restore_terminal(terminal: Option<&mut Terminal<CrosstermBackend<Stdout>>>) {
    let _ = disable_raw_mode();
    match terminal {
        Some(terminal) => {
            let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
            let _ = execute!(terminal.backend_mut(), event::DisableMouseCapture);
            let _ = execute!(terminal.backend_mut(), crossterm::cursor::Show);
            let _ = terminal.show_cursor();
        }
        None => {
            let mut out = io::stdout();
            let _ = execute!(out, LeaveAlternateScreen);
            let _ = execute!(out, event::DisableMouseCapture);
            let _ = execute!(out, crossterm::cursor::Show);
        }
    }
}

fn run_engine(terminal: &mut Terminal<CrosstermBackend<Stdout>>, path: &Path) -> Result<()> {
    let mut engine = Engine::load(path)?;

    loop {
        let (columns, rows) = size()?;
        engine.app.viewport_height = rows;
        terminal.draw(|frame| {
            render::draw_ui(
                frame,
                &engine.todo_list,
                &engine.app,
                &engine.theme,
                &engine.path,
            )
        })?;

        let area = Rect::new(0, 0, columns, rows);

        match wait_for_input(&mut engine.app)? {
            // Only a status message expired, so one repaint is enough.
            Wake::Tick => continue,
            Wake::Event(Event::Key(key)) => {
                if is_action_key(key.kind)
                    && engine.handle_key(key.code, key.modifiers) == engine::Control::Quit
                {
                    break;
                }
            }
            Wake::Event(Event::Mouse(mouse)) => {
                handle_mouse(&mut engine.app, &engine.todo_list, area, mouse);
            }
            Wake::Event(Event::Resize(_, _)) => {
                terminal.autoresize().context("resize terminal")?;
                terminal.clear().context("clear terminal after resize")?;
            }
            Wake::Event(_) => {}
        }
    }
    Ok(())
}

/// Why the event loop woke up.
enum Wake {
    /// An input event arrived.
    Event(Event),
    /// The status-message timer expired and the message was cleared.
    Tick,
}

/// How long the event loop sleeps when there is no message to expire.
///
/// A safety net rather than a tick: with nothing pending the loop blocks until
/// a key, mouse event or resize arrives, so an idle `grc` uses no CPU.
const IDLE_POLL: Duration = Duration::from_secs(3600);

/// How long the next `poll` may block: exactly until the showing message
/// expires, or [`IDLE_POLL`] when there is none.
fn poll_timeout(app: &AppState) -> Duration {
    app.message_timeout().unwrap_or(IDLE_POLL)
}

/// Wait for input, waking once when the status message expires.
///
/// `event::read` would block until the next key, so a message would stay in the
/// status bar forever; a fixed poll interval would wake up four times a second
/// for nothing. Blocking until whichever comes first — an event or the message
/// deadline — does both jobs with no idle wake-ups.
fn wait_for_input(app: &mut AppState) -> Result<Wake> {
    if app.advance(Instant::now()) {
        return Ok(Wake::Tick);
    }
    loop {
        if event::poll(poll_timeout(app))? {
            let event = event::read()?;
            // Record when the event happened, so a message it triggers counts
            // its three seconds from now rather than from the last wake-up.
            app.advance(Instant::now());
            return Ok(Wake::Event(event));
        }
        if app.advance(Instant::now()) {
            return Ok(Wake::Tick);
        }
    }
}

/// Whether a key event should run a command.
///
/// crossterm reports key releases as well as presses on some platforms
/// (notably Windows). Acting on both would run every keybinding twice, so only
/// presses and auto-repeats count.
fn is_action_key(kind: KeyEventKind) -> bool {
    kind != KeyEventKind::Release
}

fn handle_mouse(app: &mut AppState, todo_list: &TodoList, area: Rect, event: MouseEvent) {
    let MouseEvent {
        kind, column, row, ..
    } = event;

    if matches!(app.mode, Mode::Help) {
        handle_help_mouse(app, area, kind, column, row);
        return;
    }
    // A conflict has to be resolved, not navigated: only `r` and `o` mean
    // anything until the file is back in sync with the screen.
    if matches!(app.mode, Mode::Conflict) {
        return;
    }
    if app.mode.is_text_input() {
        app.clear_pending();
        return;
    }

    match kind {
        MouseEventKind::ScrollUp => scroll(app, todo_list, -1),
        MouseEventKind::ScrollDown => scroll(app, todo_list, 1),
        MouseEventKind::Down(MouseButton::Left) => {
            handle_click(app, todo_list, area, column, row);
        }
        _ => {}
    }
}

fn handle_help_mouse(app: &mut AppState, area: Rect, kind: MouseEventKind, column: u16, row: u16) {
    match kind {
        MouseEventKind::ScrollUp => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }
        MouseEventKind::ScrollDown => {
            app.help_scroll = app
                .help_scroll
                .saturating_add(1)
                .min(render::help::max_scroll(app.viewport_height));
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let popup = help_popup(area);
            let inside = column >= popup.x
                && column < popup.x.saturating_add(popup.width)
                && row >= popup.y
                && row < popup.y.saturating_add(popup.height);
            if !inside {
                app.mode = Mode::Normal;
            }
        }
        _ => {}
    }
}

/// Geometry of the help overlay, shared with the renderer so mouse hit-testing
/// can never disagree with what was drawn.
pub(crate) fn help_popup(area: Rect) -> Rect {
    widgets::popup::centered(area, HELP_POPUP_WIDTH_PERCENT, HELP_POPUP_HEIGHT)
}

fn handle_click(app: &mut AppState, todo_list: &TodoList, area: Rect, column: u16, row: u16) {
    // Row 0 is a panel border and the last two rows are the hint and status
    // bars, so a click only names a list row between them.
    let Some(list_row) = row.checked_sub(1).map(usize::from) else {
        return;
    };
    if row + 2 >= area.height {
        return;
    }

    // The panels sit above the hint and status bars; splitting the full area
    // would give them two rows too many and misplace every scrolled click.
    let panel_area = Rect::new(0, 0, area.width, area.height.saturating_sub(2));
    let [left, right] = render::panel_split(panel_area);
    if column < left.x + left.width {
        app.focus = Focus::Left;
        let total = app.tree_items.len();
        let index = list_row + visible_offset(app.left_state.selected(), total, left);
        if index < total {
            app.left_state.select(Some(index));
        }
        return;
    }

    let node = selected_node(app);
    let row_count = node.as_ref().map_or(0, |node| {
        get_task_refs_filtered(todo_list, node, &app.filter_lower, app.hide_done).len()
    });
    app.focus = Focus::Right;
    let index = list_row + visible_offset(app.right_state.selected(), row_count, right);
    if index < row_count {
        app.right_state.select(Some(index));
        return;
    }
    clamp_right_selection(app, row_count);
}

/// First visible list row of a panel, matching what the renderer draws.
///
/// Clicking must resolve against the scrolled view, not the raw document, so a
/// click on a scrolled list selects the row the user actually sees.
fn visible_offset(selected: Option<usize>, total: usize, panel: Rect) -> usize {
    let height = usize::from(panel.height.saturating_sub(2));
    selected.map_or(0, |selected| widgets::list_offset(total, selected, height))
}

fn scroll(app: &mut AppState, todo_list: &TodoList, delta: isize) {
    match app.focus {
        Focus::Left => {
            if app.tree_items.is_empty() {
                app.left_state.select(None);
                return;
            }
            let current = app.left_state.selected().unwrap_or(0) as isize;
            let max = app.tree_items.len().saturating_sub(1) as isize;
            app.left_state
                .select(Some(current.saturating_add(delta).clamp(0, max) as usize));
        }
        Focus::Right => {
            let Some(node) = selected_node(app) else {
                app.right_state.select(None);
                return;
            };
            let refs = get_task_refs_filtered(todo_list, &node, &app.filter_lower, app.hide_done);
            if refs.is_empty() {
                app.right_state.select(None);
                return;
            }
            let current = app.right_state.selected().unwrap_or(0) as isize;
            let max = refs.len().saturating_sub(1) as isize;
            app.right_state
                .select(Some(current.saturating_add(delta).clamp(0, max) as usize));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_does_not_overflow() {
        assert_eq!(widgets::percentage(u16::MAX, 55), 36044);
        assert_eq!(widgets::percentage(100, 27), 27);
    }

    fn click_area() -> Rect {
        // 14 rows leaves a 12-row panel, so ten list rows fit inside the border.
        Rect::new(0, 0, 40, 14)
    }

    fn state_with_tasks(count: usize) -> (TodoList, AppState) {
        let mut content = String::from("# main\n");
        for index in 0..count {
            content.push_str(&format!("- [ ] task {index}\n"));
        }
        let todo = crate::parser::parse_str(&content);
        let mut app = AppState::new(1);
        app.tree_items = crate::tui::tree::build_visible_tree_items(
            &todo,
            &app.mode,
            &app.filter_lower,
            &app.folded,
            app.hide_done,
        );
        (todo, app)
    }

    #[test]
    fn the_loop_blocks_until_the_next_message_deadline() {
        let mut app = AppState::new(1);
        let start = Instant::now();
        app.advance(start);
        // Nothing to expire: sleep until an event arrives.
        assert_eq!(poll_timeout(&app), IDLE_POLL);
        assert!(
            IDLE_POLL >= Duration::from_secs(60),
            "a short idle timeout would wake the loop for nothing"
        );

        app.set_info("saved");
        let timeout = poll_timeout(&app);
        assert!(timeout > Duration::ZERO && timeout <= state::MESSAGE_TTL);

        app.advance(start + state::MESSAGE_TTL);
        assert!(app.message.is_none());
        assert_eq!(poll_timeout(&app), IDLE_POLL);
    }

    #[test]
    fn key_releases_do_not_run_commands_again() {
        assert!(is_action_key(KeyEventKind::Press));
        assert!(is_action_key(KeyEventKind::Repeat));
        assert!(!is_action_key(KeyEventKind::Release));
    }

    #[test]
    fn click_resolves_against_the_scrolled_view() {
        let (todo, mut app) = state_with_tasks(20);
        app.focus = Focus::Right;
        // Selecting row 15 scrolls the list so that row 6 is the first visible one.
        app.right_state.select(Some(15));
        let [_, right] = render::panel_split(Rect::new(0, 0, 40, 12));
        assert_eq!(visible_offset(Some(15), 20, right), 6);

        handle_click(&mut app, &todo, click_area(), right.x + 2, 1);
        assert_eq!(app.focus, Focus::Right);
        assert_eq!(app.right_state.selected(), Some(6));
    }

    #[test]
    fn click_uses_the_same_panel_boundary_as_the_renderer() {
        let (todo, mut app) = state_with_tasks(1);
        app.focus = Focus::Right;
        let area = click_area();
        let [left, right] = render::panel_split(area);
        assert_eq!(left.width + right.width, area.width);

        // The last cell of the left panel, then the first cell of the right one.
        handle_click(&mut app, &todo, area, left.width - 1, 1);
        assert_eq!(app.focus, Focus::Left);
        handle_click(&mut app, &todo, area, left.width, 1);
        assert_eq!(app.focus, Focus::Right);
    }

    #[test]
    fn conflict_mode_ignores_the_mouse_until_it_is_resolved() {
        let (todo, mut app) = state_with_tasks(5);
        app.mode = Mode::Conflict;
        app.focus = Focus::Right;
        app.right_state.select(Some(2));

        let area = click_area();
        let mouse = |kind, column, row| MouseEvent {
            kind,
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        for event in [
            mouse(MouseEventKind::ScrollDown, 5, 3),
            mouse(MouseEventKind::ScrollUp, 5, 3),
            mouse(MouseEventKind::Down(MouseButton::Left), 30, 2),
        ] {
            handle_mouse(&mut app, &todo, area, event);
        }
        assert_eq!(app.mode, Mode::Conflict);
        assert_eq!(app.focus, Focus::Right);
        assert_eq!(app.right_state.selected(), Some(2));
    }

    #[test]
    fn clicks_on_the_chrome_or_borders_are_ignored() {
        let (todo, mut app) = state_with_tasks(3);
        app.focus = Focus::Right;
        app.right_state.select(Some(2));
        for row in [0, 13] {
            handle_click(&mut app, &todo, click_area(), 5, row);
        }
        assert_eq!(app.right_state.selected(), Some(2));
    }

    #[test]
    fn help_popup_matches_the_rendered_overlay() {
        let area = Rect::new(0, 0, 80, 24);
        let popup = help_popup(area);
        assert_eq!(popup, render::help::overlay_area(area));
        assert!(popup.width <= area.width && popup.height <= area.height);
    }

    #[test]
    fn help_popup_degenerates_on_tiny_terminals() {
        for size in [(0, 0), (1, 1), (10, 3)] {
            let area = Rect::new(0, 0, size.0, size.1);
            let popup = help_popup(area);
            assert!(popup.width <= size.0 && popup.height <= size.1);
        }
    }
}
