use crate::task::TodoList;
use crate::tui::count_tasks_in_section;
use crate::tui::state::{AppState, Focus, Mode, TreeNode};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

pub fn draw_left(f: &mut Frame, todo_list: &TodoList, app: &mut AppState, area: Rect) {
    let focused = app.focus == Focus::Left && matches!(app.mode, Mode::Normal);
    let renaming = matches!(
        app.mode,
        Mode::InputSection { .. } | Mode::InputSubsection { .. }
    );

    let border_color = if renaming || focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " Workspaces ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(border_color));

    let renaming_node = if let Mode::InputSection { node: Some(n), .. } = &app.mode {
        Some(*n)
    } else {
        None
    };
    let rename_buf = if let Mode::InputSection { buf, .. } = &app.mode {
        Some(buf.clone())
    } else {
        None
    };

    let (sec_ghost_node, sec_ghost_buf) = if let Mode::InputSection {
        node: None, buf, ..
    } = &app.mode
    {
        (
            Some(TreeNode::Section(todo_list.sections.len())),
            Some(buf.clone()),
        )
    } else {
        (None, None)
    };

    let (sub_ghost_node, sub_ghost_buf) = if let Mode::InputSubsection {
        parent_sec_idx,
        buf,
        ..
    } = &app.mode
    {
        let sub_len = todo_list.sections[*parent_sec_idx].subsections.len();
        (
            Some(TreeNode::Subsection(*parent_sec_idx, sub_len)),
            Some(buf.clone()),
        )
    } else {
        (None, None)
    };

    let items: Vec<ListItem> = app
        .tree_nodes
        .iter()
        .map(|node| {
            if Some(*node) == sec_ghost_node {
                let buf = sec_ghost_buf.as_deref().unwrap_or("");
                ListItem::new(Line::from(vec![
                    Span::styled(
                        "  ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(buf.to_string(), Style::default().fg(Color::White)),
                    Span::styled(
                        "█",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::SLOW_BLINK),
                    ),
                ]))
            } else if Some(*node) == renaming_node {
                let buf = rename_buf.as_deref().unwrap_or("");
                match node {
                    TreeNode::Section(_s) => {
                        let arrow = "  ";
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                arrow,
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(buf.to_string(), Style::default().fg(Color::White)),
                            Span::styled(
                                "█",
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::SLOW_BLINK),
                            ),
                        ]))
                    }
                    TreeNode::Subsection(_s, _sb) => {
                        let connector = "    ";
                        ListItem::new(Line::from(vec![
                            Span::styled(connector, Style::default().fg(Color::Blue)),
                            Span::styled(buf.to_string(), Style::default().fg(Color::White)),
                            Span::styled(
                                "█",
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::SLOW_BLINK),
                            ),
                        ]))
                    }
                }
            } else if Some(*node) == sub_ghost_node {
                let buf = sub_ghost_buf.as_deref().unwrap_or("");
                match node {
                    TreeNode::Subsection(_s, _sb) => {
                        let connector = "    ";
                        ListItem::new(Line::from(vec![
                            Span::styled(connector, Style::default().fg(Color::Blue)),
                            Span::styled(buf.to_string(), Style::default().fg(Color::White)),
                            Span::styled(
                                "█",
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::SLOW_BLINK),
                            ),
                        ]))
                    }
                    _ => ListItem::new(Line::from("")),
                }
            } else {
                match node {
                    TreeNode::Section(s) => {
                        let sec = &todo_list.sections[*s];
                        let arrow = "  ";
                        let n = count_tasks_in_section(sec);
                        ListItem::new(Line::from(vec![Span::styled(
                            format!("{}{} ({})", arrow, sec.name, n),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )]))
                    }
                    TreeNode::Subsection(s, sb) => {
                        let sec = &todo_list.sections[*s];
                        let sub = &sec.subsections[*sb];
                        let connector = "    ";
                        ListItem::new(Line::from(vec![Span::styled(
                            format!("{}{} ({})", connector, sub.name, sub.tasks.len()),
                            Style::default().fg(Color::Blue),
                        )]))
                    }
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
