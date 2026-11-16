use crate::task::TodoList;
use crate::tui::state::{AppState, Focus, Mode, TreeItem};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

pub fn draw_left(f: &mut Frame, todo_list: &TodoList, app: &mut AppState, area: Rect) {
    let focused = app.focus == Focus::Left && matches!(app.mode, Mode::Normal);
    let renaming = matches!(app.mode, Mode::InputSection(_));

    let border_color = if renaming || focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = if app.filter.is_empty() {
        " Workspaces "
    } else {
        " Workspaces (filtered) "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(border_color));

    let (renaming_node, rename_field) = match &app.mode {
        Mode::InputSection(input) => (input.node.as_ref(), Some(&input.field)),
        _ => (None, None),
    };

    let ghost = match &app.mode {
        Mode::InputSection(input) if input.node.is_none() => Some(&input.field),
        _ => None,
    };

    let items: Vec<ListItem> = app
        .tree_items
        .iter()
        .map(|item| match item {
            TreeItem::Ghost(depth) => {
                let (buf, cursor) =
                    ghost.map_or(("", 0), |field| (field.buf.as_str(), field.cursor));
                let (before, after) = crate::tui::render::split_at_char(buf, cursor);
                ListItem::new(Line::from(vec![
                    Span::styled(indent(*depth), Style::default().fg(Color::Green)),
                    Span::styled(before, Style::default().fg(Color::White)),
                    Span::styled(
                        "|",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::SLOW_BLINK),
                    ),
                    Span::styled(after, Style::default().fg(Color::White)),
                ]))
            }
            TreeItem::Node(path) => {
                let depth = path.len().saturating_sub(1);
                if Some(path) == renaming_node {
                    let field = rename_field.expect("rename field exists for rename node");
                    let (before, after) =
                        crate::tui::render::split_at_char(&field.buf, field.cursor);
                    ListItem::new(Line::from(vec![
                        Span::styled(indent(depth), Style::default().fg(Color::Cyan)),
                        Span::styled(before, Style::default().fg(Color::White)),
                        Span::styled(
                            "|",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::SLOW_BLINK),
                        ),
                        Span::styled(after, Style::default().fg(Color::White)),
                    ]))
                } else if let Some(sec) = crate::task::get_node(todo_list, path) {
                    let n = sec.count_tasks();
                    let (fg, modifier) = if depth == 0 {
                        (Color::Yellow, Modifier::BOLD)
                    } else {
                        (Color::Blue, Modifier::empty())
                    };
                    ListItem::new(Line::from(vec![Span::styled(
                        format!("{}{} ({n})", indent(depth), sec.name),
                        Style::default().fg(fg).add_modifier(modifier),
                    )]))
                } else {
                    ListItem::new(Line::from(""))
                }
            }
        })
        .collect();

    let hl = if focused {
        Style::default()
            .bg(Color::Rgb(20, 60, 95))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(Color::Rgb(38, 38, 50))
            .fg(Color::Rgb(160, 160, 160))
    };

    let list = List::new(items)
        .block(block)
        .highlight_style(hl)
        .highlight_symbol("");

    f.render_stateful_widget(list, area, &mut app.left_state);
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}
