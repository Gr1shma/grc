use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup = centered_rect(65, 46, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " Keybindings Help ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(Color::Yellow));

    let content = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Global Commands:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("    q            - Quit application"),
        Line::from("    ?            - Toggle this help menu"),
        Line::from("    Tab          - Switch panel focus (Left / Right)"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Left Panel (Sections):",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("    j / Down     - Move down the tree navigator"),
        Line::from("    k / Up       - Move up the tree navigator"),
        Line::from("    gg           - Jump to the top of the list"),
        Line::from("    G            - Jump to the bottom of the list"),
        Line::from("    l / Enter    - Focus right task list"),
        Line::from("    A            - Add new top-level section"),
        Line::from("    a / o        - Add new subsection below current item"),
        Line::from("    O            - Add new subsection above current item"),
        Line::from("    i            - Rename selected section or subsection"),
        Line::from("    dd           - Delete selected section or subsection instantly"),
        Line::from("    yy / p       - Yank / paste item below (after current)"),
        Line::from("    P            - Paste item above (one step up, like Shift+O)"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Right Panel (Tasks):",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("    j / Down     - Move down the task list"),
        Line::from("    k / Up       - Move up the task list"),
        Line::from("    gg           - Jump to the top of the list"),
        Line::from("    G            - Jump to the bottom of the list"),
        Line::from("    h / Esc      - Move focus back to sections tree"),
        Line::from("    a / o        - Add new task below"),
        Line::from("    O            - Add new task above"),
        Line::from("    i            - Edit selected task text inline"),
        Line::from("    t            - Set due date (empty = clear, Enter to confirm)"),
        Line::from("    x / Space    - Toggle task status (Done / Todo)"),
        Line::from("    yy / p       - Yank / paste item below (after current)"),
        Line::from("    P            - Paste item above (one step up, like Shift+O)"),
        Line::from("    dd           - Delete selected task"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  While Typing (any input mode):",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("    |            - Bar cursor marks your insert position"),
        Line::from("    Left / Right - Move cursor within the text (vim-style)"),
        Line::from("    Home / End   - Jump to start / end of the text"),
        Line::from("    Backspace    - Delete character before cursor"),
        Line::from("    Delete       - Delete character at cursor"),
        Line::from("    Enter        - Confirm input"),
        Line::from("    Esc          - Cancel input"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Due Date Formats (used with t):",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("    YYYY-MM-DD              - Absolute  e.g. 2025-12-31"),
        Line::from("    today  /  tod  /  t     - Today"),
        Line::from("    tomorrow  /  tmr  /  tmw  /  tom  - Tomorrow"),
        Line::from("    next week  /  nextweek  /  nw     - 7 days from today"),
        Line::from(""),
        Line::from("  Press Esc, q, or ? to close this help window"),
    ];

    let para = Paragraph::new(content).block(block);
    f.render_widget(para, popup);
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width: popup_width.min(area.width),
        height: height.min(area.height),
    }
}
