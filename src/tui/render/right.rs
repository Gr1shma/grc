use crate::task::{Task, TodoList};
use crate::tui::state::{AppState, Focus, Mode, TreeNode};
use crate::tui::{get_task_from_ref, get_task_refs, selected_node, TaskRef};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn draw_right(f: &mut Frame, todo_list: &TodoList, app: &mut AppState, area: Rect) {
    let focused_normal = app.focus == Focus::Right && matches!(app.mode, Mode::Normal);
    let in_task_input = matches!(app.mode, Mode::InputTask { .. } | Mode::InputDue { .. });

    let title = selected_node(app)
        .map(|node| match node {
            TreeNode::Section(s) => format!(
                "  {}  ",
                todo_list
                    .sections
                    .get(s)
                    .map(|sec| sec.name.as_str())
                    .unwrap_or("")
            ),
            TreeNode::Subsection(s, sb) => {
                if s < todo_list.sections.len() {
                    let sec = &todo_list.sections[s];
                    if sb < sec.subsections.len() {
                        format!("  {} › {}  ", sec.name, sec.subsections[sb].name)
                    } else {
                        format!("  {} › New Subsection  ", sec.name)
                    }
                } else {
                    " Tasks ".to_string()
                }
            }
        })
        .unwrap_or_else(|| " Tasks ".to_string());

    let border_color = if in_task_input || focused_normal {
        Color::Green
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(border_color));

    let task_refs = selected_node(app)
        .map(|node| get_task_refs(todo_list, node))
        .unwrap_or_default();

    let inner_w = area.width.saturating_sub(6) as usize;

    let editing_idx = if let Mode::InputTask { editing_idx, .. } = &app.mode {
        *editing_idx
    } else {
        None
    };
    let edit_buf = if let Mode::InputTask { buf, .. } = &app.mode {
        Some(buf.clone())
    } else {
        None
    };
    let due_editing_idx = if let Mode::InputDue { task_idx, .. } = &app.mode {
        Some(*task_idx)
    } else {
        None
    };
    let due_buf = if let Mode::InputDue { buf, .. } = &app.mode {
        Some(buf.clone())
    } else {
        None
    };

    let mut items: Vec<ListItem> = task_refs
        .iter()
        .enumerate()
        .map(|(i, ref_item)| {
            let task = get_task_from_ref(todo_list, ref_item);
            let sub_name = if matches!(selected_node(app), Some(TreeNode::Section(_))) {
                match ref_item {
                    TaskRef::SubsectionTask { sec_idx, sub_idx, .. } => {
                        todo_list.sections.get(*sec_idx)
                            .and_then(|s| s.subsections.get(*sub_idx))
                            .map(|sub| sub.name.as_str())
                    }
                    _ => None,
                }
            } else {
                None
            };

            if Some(i) == editing_idx {
                render_editing_row(edit_buf.as_deref().unwrap_or(""), task.is_done, inner_w, sub_name)
            } else if Some(i) == due_editing_idx {
                render_due_editing_row(task, due_buf.as_deref().unwrap_or(""), inner_w, sub_name)
            } else {
                render_task_item(task, inner_w, sub_name)
            }
        })
        .collect();

    if let Mode::InputTask {
        editing_idx: None,
        insert_idx,
        ref buf,
    } = app.mode
    {
        let ghost = render_ghost_row(buf, inner_w);
        let pos = insert_idx.unwrap_or(items.len()).min(items.len());
        items.insert(pos, ghost);
    }

    if items.is_empty() {
        let hint = Paragraph::new(Line::from(vec![
            Span::styled("  No tasks — press ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "a",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to add one", Style::default().fg(Color::DarkGray)),
        ]))
        .block(block);
        f.render_widget(hint, area);
        return;
    }

    let hl = if focused_normal || in_task_input {
        Style::default()
            .bg(Color::Rgb(18, 65, 28))
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

    f.render_stateful_widget(list, area, &mut app.right_state);
}

fn render_task_item(task: &Task, width: usize, sub_name: Option<&str>) -> ListItem<'static> {
    let (box_ch, box_style) = if task.is_done {
        ("  x ", Style::default().fg(Color::Green))
    } else {
        ("  ○ ", Style::default().fg(Color::DarkGray))
    };

    let text_style = if task.is_done {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::CROSSED_OUT)
        } else {
            Style::default().fg(Color::Reset)
        };

    let due_info: Option<(String, Style)> = task.due.map(|d| {
        let today = chrono::Local::now().date_naive();
        let label = format!("  due:{}", d.format("%Y-%m-%d"));
        let style = if task.is_done {
            Style::default().fg(Color::DarkGray)
        } else if d < today {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if d == today {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Blue)
        };
        (label, style)
    });

    let sub_len = sub_name.map(|s| s.len() + 3).unwrap_or(0);
    let text_len = box_ch.len() + sub_len + task.text.len();
    let due_len = due_info.as_ref().map(|(s, _)| s.len()).unwrap_or(0);
    let pad = " ".repeat(width.saturating_sub(text_len + due_len));

    let mut spans = vec![
        Span::styled(box_ch, box_style),
    ];
    if let Some(name) = sub_name {
        spans.push(Span::styled(format!("[{}] ", name), Style::default().fg(Color::Magenta)));
    }
    spans.push(Span::styled(task.text.clone(), text_style));
    spans.push(Span::raw(pad));
    if let Some((label, style)) = due_info {
        spans.push(Span::styled(label, style));
    }
    ListItem::new(Line::from(spans))
}

fn render_editing_row(buf: &str, is_done: bool, _width: usize, sub_name: Option<&str>) -> ListItem<'static> {
    let check = if is_done {
        Span::styled("  x ", Style::default().fg(Color::Green))
    } else {
        Span::styled("  ○ ", Style::default().fg(Color::DarkGray))
    };
    let mut spans = vec![check];
    if let Some(name) = sub_name {
        spans.push(Span::styled(format!("[{}] ", name), Style::default().fg(Color::Magenta)));
    }
    spans.push(Span::styled(buf.to_string(), Style::default().fg(Color::White)));
    spans.push(Span::styled(
        "█",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::SLOW_BLINK),
    ));
    ListItem::new(Line::from(spans))
}

fn render_due_editing_row(task: &Task, due_buf: &str, width: usize, sub_name: Option<&str>) -> ListItem<'static> {
    let (box_ch, box_style) = if task.is_done {
        ("  x ", Style::default().fg(Color::Green))
    } else {
        ("  ○ ", Style::default().fg(Color::DarkGray))
    };
    let text_style = Style::default().fg(Color::Reset);
    let due_prefix = "  due:";
    let sub_len = sub_name.map(|s| s.len() + 3).unwrap_or(0);
    let text_len = box_ch.len() + sub_len + task.text.len();
    let due_len = due_prefix.len() + due_buf.len() + 1;
    let pad = " ".repeat(width.saturating_sub(text_len + due_len));

    let mut spans = vec![
        Span::styled(box_ch, box_style),
    ];
    if let Some(name) = sub_name {
        spans.push(Span::styled(format!("[{}] ", name), Style::default().fg(Color::Magenta)));
    }
    spans.push(Span::styled(task.text.clone(), text_style));
    spans.push(Span::raw(pad));
    spans.push(Span::styled(due_prefix, Style::default().fg(Color::Blue)));
    spans.push(Span::styled(due_buf.to_string(), Style::default().fg(Color::White)));
    spans.push(Span::styled(
        "█",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::SLOW_BLINK),
    ));
    ListItem::new(Line::from(spans))
}

fn render_ghost_row(buf: &str, _width: usize) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled("  ○ ", Style::default().fg(Color::DarkGray)),
        Span::styled(buf.to_string(), Style::default().fg(Color::White)),
        Span::styled(
            "█",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]))
}
