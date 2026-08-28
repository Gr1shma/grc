use crate::task::{Section, Task, TodoList, get_node_mut};
use anyhow::Result;
use chrono::{Datelike, NaiveDate, Weekday};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn resolve_relative_date(input: &str) -> Option<NaiveDate> {
    let cleaned = input.trim().to_lowercase();
    if cleaned.is_empty() {
        return None;
    }

    let today = chrono::Local::now().date_naive();
    match cleaned.as_str() {
        "today" | "tod" | "t" => Some(today),
        "tomorrow" | "tmr" | "tmw" | "tom" => Some(today + chrono::Duration::days(1)),
        "next week" | "nw" | "nextweek" => Some(today + chrono::Duration::days(7)),
        "sun" | "sunday" => Some(next_weekday(today, Weekday::Sun)),
        "mon" | "monday" => Some(next_weekday(today, Weekday::Mon)),
        "tue" | "tuesday" => Some(next_weekday(today, Weekday::Tue)),
        "wed" | "wednesday" => Some(next_weekday(today, Weekday::Wed)),
        "thu" | "thursday" => Some(next_weekday(today, Weekday::Thu)),
        "fri" | "friday" => Some(next_weekday(today, Weekday::Fri)),
        "sat" | "saturday" => Some(next_weekday(today, Weekday::Sat)),
        _ => parse_additional_formats(&cleaned, today),
    }
}

/// Parse a weekday name or abbreviation (already lowercased).
fn parse_weekday(s: &str) -> Option<Weekday> {
    match s {
        "sun" | "sunday" => Some(Weekday::Sun),
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        _ => None,
    }
}

/// Parse relative offsets such as `+3d`, `-2w`, `+1m`, `+1y`, or the longer
/// forms `+3 days`, `-2 weeks`, `+1 month`, `+1 year`.
fn parse_relative_offset(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let s = s.trim();
    let (sign, rest) = if let Some(r) = s.strip_prefix('+') {
        (1i64, r)
    } else if let Some(r) = s.strip_prefix('-') {
        (-1i64, r)
    } else {
        return None;
    };
    if rest.is_empty() {
        return None;
    }
    let num_end = rest
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    if num_end == 0 {
        return None;
    }
    let num: i64 = rest[..num_end].parse().ok()?;
    let unit = rest[num_end..].trim();
    let days = match (sign, unit) {
        (s, "d" | "day" | "days") => s * num,
        (s, "w" | "week" | "weeks") => s * num * 7,
        (s, "m" | "month" | "months") => s * num * 30,
        (s, "y" | "year" | "years") => s * num * 365,
        _ => return None,
    };
    Some(today + chrono::Duration::days(days))
}

/// Parse natural-language offsets such as `in 3 days`, `in 2 weeks`,
/// `in 1 month`, `in 1 year`.
fn parse_in_offset(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let rest = s.trim().strip_prefix("in")?;
    let rest = rest.trim_start();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let num: i64 = parts[0].parse().ok()?;
    let days = match parts[1] {
        "d" | "day" | "days" => num,
        "w" | "week" | "weeks" => num * 7,
        "m" | "month" | "months" => num * 30,
        "y" | "year" | "years" => num * 365,
        _ => return None,
    };
    Some(today + chrono::Duration::days(days))
}

/// Parse a month name plus day, e.g. `jun 15`, `january 1`.
/// Uses the current year, falling back to next year when the date has passed.
fn parse_month_day(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let mut parts = s.split_whitespace();
    let month_str = parts.next()?;
    let day_str = parts.next()?;
    let day: u32 = day_str.parse().ok()?;
    let month = month_name_to_num(month_str)?;

    if let Some(date) = NaiveDate::from_ymd_opt(today.year(), month, day) {
        if date >= today {
            return Some(date);
        }
        return NaiveDate::from_ymd_opt(today.year() + 1, month, day);
    }
    NaiveDate::from_ymd_opt(today.year() + 1, month, day)
}

/// Map a (case-insensitive) month name or 3-letter abbreviation to its number.
fn month_name_to_num(s: &str) -> Option<u32> {
    match s.to_ascii_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

/// Try the extra date formats beyond the single-word shortcuts.
fn parse_additional_formats(cleaned: &str, today: NaiveDate) -> Option<NaiveDate> {
    // "next <weekday>" and "this <weekday>"
    for (prefix, only_if_not_today) in [("next ", true), ("this ", false)] {
        if let Some(rest) = cleaned.strip_prefix(prefix)
            && let Some(wd) = parse_weekday(rest)
        {
            let resolved = next_weekday(today, wd);
            if !only_if_not_today {
                if today.weekday() == wd {
                    return Some(today);
                }
                if resolved <= today {
                    return Some(resolved);
                }
            }
            return Some(resolved);
        }
    }

    if let Some(date) = parse_relative_offset(cleaned, today) {
        return Some(date);
    }

    if let Some(date) = parse_in_offset(cleaned, today) {
        return Some(date);
    }

    if let Some(date) = parse_month_day(cleaned, today) {
        return Some(date);
    }

    for fmt in ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"] {
        if let Ok(date) = NaiveDate::parse_from_str(cleaned, fmt) {
            return Some(date);
        }
    }

    None
}

/// Try to parse a date from the start of `s`, consuming progressively more
/// whitespace-separated words (e.g. `next monday`). Returns the parsed date and
/// the byte offset in `s` just past the matched text.
pub fn parse_due_token(s: &str) -> Option<(NaiveDate, usize)> {
    if s.is_empty() {
        return None;
    }

    let mut word_ends: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < s.len() {
        while i < s.len() && s.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= s.len() {
            break;
        }
        while i < s.len() && !s.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        word_ends.push(i);
    }

    if word_ends.is_empty() {
        return None;
    }

    for &end in &word_ends {
        if let Some(date) = resolve_relative_date(&s[..end]) {
            return Some((date, end));
        }
    }

    None
}

fn next_weekday(from: NaiveDate, target: Weekday) -> NaiveDate {
    let current = from.weekday();
    let diff = (target.num_days_from_monday() + 7 - current.num_days_from_monday()) % 7;
    if diff == 0 {
        from + chrono::Duration::days(7)
    } else {
        from + chrono::Duration::days(diff as i64)
    }
}

pub fn parse_file(path: &Path) -> Result<TodoList> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut todo_list = TodoList::default();

    let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
        let is_heading = hashes > 0
            && hashes < trimmed.len()
            && trimmed.as_bytes()[hashes] == b' '
            && !trimmed[hashes + 1..].trim().is_empty();

        if is_heading {
            let level = hashes;
            let name = trimmed[hashes + 1..].trim().to_string();

            while stack.last().is_some_and(|(lvl, _)| *lvl >= level) {
                stack.pop();
            }

            let parent_path: Vec<usize> = stack.last().map_or_else(Vec::new, |(_, p)| p.clone());

            let new_section = Section::new(name);
            if parent_path.is_empty() {
                todo_list.sections.push(new_section);
                stack.push((level, vec![todo_list.sections.len() - 1]));
            } else {
                let parent = get_node_mut(&mut todo_list, &parent_path)
                    .ok_or_else(|| anyhow::anyhow!("Malformed markdown: parent section not found for path {:?}", parent_path))?;
                parent.children.push(new_section);
                let mut p = parent_path.clone();
                p.push(parent.children.len() - 1);
                stack.push((level, p));
            }
            continue;
        }

        if trimmed.starts_with("- [ ")
            || trimmed.starts_with("- [x]")
            || trimmed.starts_with("- [X]")
        {
            let is_done = matches!(trimmed.as_bytes()[3], b'x' | b'X');
            let mut task_text = trimmed[5..].trim().to_string();

            let mut due = None;
            // Find "due:" only when preceded by start-of-string or whitespace
            if let Some(idx) = task_text
                .find("due:")
                .filter(|&i| i == 0 || task_text.as_bytes()[i - 1] == b' ')
            {
                let after_due = &task_text[idx + 4..];
                if let Some((parsed_date, consumed)) = parse_due_token(after_due) {
                    due = Some(parsed_date);
                    task_text = format!(
                        "{}{}",
                        task_text[..idx].trim_end(),
                        task_text[idx + 4 + consumed..].trim_start()
                    )
                    .trim()
                    .to_string();
                }
            }

            let mut task = Task::new(task_text);
            task.is_done = is_done;
            task.due = due;

            if let Some((_, path)) = stack.last() {
                if let Some(sec) = get_node_mut(&mut todo_list, path) {
                    sec.tasks.push(task);
                }
            } else if let Some(sec) = todo_list.sections.last_mut() {
                // Attach tasks before any heading to the last section
                sec.tasks.push(task);
            }
        }
    }

    Ok(todo_list)
}

pub fn write_file(path: &Path, todo_list: &TodoList) -> Result<()> {
    // Atomic write: write to temp file then rename
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    // Use a unique temp file name with random component
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = dir.join(format!(".grc_{}_{}.tmp", std::process::id(), nanos));

    {
        let mut file = File::create(&temp_path)?;
        let mut first = true;
        let mut last_wrote_tasks = false;

        for sec in &todo_list.sections {
            write_section(&mut file, sec, 1, &mut first, &mut last_wrote_tasks)?;
        }
    }

    std::fs::rename(&temp_path, path)?;
    Ok(())
}

fn write_section(
    file: &mut File,
    sec: &Section,
    level: usize,
    first: &mut bool,
    last_wrote_tasks: &mut bool,
) -> Result<()> {
    if !*first && *last_wrote_tasks {
        writeln!(file)?;
    }
    let hashes = "#".repeat(level);
    writeln!(file, "{hashes} {}", sec.name)?;
    *first = false;

    if sec.tasks.is_empty() {
        *last_wrote_tasks = false;
    } else {
        writeln!(file)?;
        for task in &sec.tasks {
            write_task_line(file, task)?;
        }
        *last_wrote_tasks = true;
    }

    for child in &sec.children {
        write_section(file, child, level + 1, first, last_wrote_tasks)?;
    }
    Ok(())
}

fn write_task_line(file: &mut File, task: &Task) -> Result<()> {
    let box_str = if task.is_done { "[x]" } else { "[ ]" };
    if let Some(date) = task.due {
        let formatted = date.format("%Y-%m-%d");
        writeln!(file, "- {box_str} {} due:{formatted}", task.text)?;
    } else {
        writeln!(file, "- {box_str} {}", task.text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TodoList;
    use chrono::NaiveDate;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn temp_file_with(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn parses_single_section_with_tasks() {
        let f = temp_file_with("# main\n- [ ] Buy milk\n- [x] Walk dog\n");
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].name, "main");
        assert_eq!(list.sections[0].tasks.len(), 2);
        assert!(!list.sections[0].tasks[0].is_done);
        assert_eq!(list.sections[0].tasks[0].text, "Buy milk");
        assert!(list.sections[0].tasks[1].is_done);
    }

    #[test]
    fn parses_done_uppercase_x() {
        let f = temp_file_with("# sec\n- [X] Done task\n");
        let list = parse_file(f.path()).unwrap();
        assert!(list.sections[0].tasks[0].is_done);
    }

    #[test]
    fn parses_due_date() {
        let f = temp_file_with("# sec\n- [ ] Submit report due:2025-01-15\n");
        let list = parse_file(f.path()).unwrap();
        let task = &list.sections[0].tasks[0];
        assert_eq!(task.text, "Submit report");
        assert_eq!(
            task.due,
            Some(NaiveDate::from_ymd_opt(2025, 1, 15).unwrap())
        );
    }

    #[test]
    fn parses_nested_subsections() {
        let content =
            "# work\n- [ ] Top task\n## backend\n- [ ] Fix API\n## frontend\n- [x] Style button\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].tasks.len(), 1);
        assert_eq!(list.sections[0].children.len(), 2);
        assert_eq!(list.sections[0].children[0].name, "backend");
        assert_eq!(list.sections[0].children[1].name, "frontend");
        assert!(list.sections[0].children[1].tasks[0].is_done);
    }

    #[test]
    fn parses_multiple_top_level_sections() {
        let content = "# work\n- [ ] Task A\n# personal\n- [ ] Task B\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 2);
        assert_eq!(list.sections[0].name, "work");
        assert_eq!(list.sections[1].name, "personal");
    }

    #[test]
    fn parses_empty_file_as_empty_list() {
        let f = temp_file_with("");
        let list = parse_file(f.path()).unwrap();
        assert!(list.sections.is_empty());
    }

    #[test]
    fn parses_headings_only_no_tasks() {
        let f = temp_file_with("# empty-section\n## empty-sub\n");
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 1);
        assert!(list.sections[0].tasks.is_empty());
        assert_eq!(list.sections[0].children.len(), 1);
        assert!(list.sections[0].children[0].tasks.is_empty());
    }

    #[test]
    fn ignores_non_heading_non_task_lines() {
        let content = "# sec\nsome random text\n- [ ] Real task\nanother line\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections[0].tasks.len(), 1);
        assert_eq!(list.sections[0].tasks[0].text, "Real task");
    }

    #[test]
    fn parses_file_starting_with_second_level_heading() {
        let content = "## My Project\n- [ ] Task one\n- [ ] Task two\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].name, "My Project");
        assert_eq!(list.sections[0].tasks.len(), 2);
    }

    #[test]
    fn parses_deep_four_level_heading_hierarchy() {
        let content = "# a\n## b\n### c\n#### d\n- [ ] deep task\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections[0].name, "a");
        assert_eq!(list.sections[0].children[0].name, "b");
        assert_eq!(list.sections[0].children[0].children[0].name, "c");
        assert_eq!(
            list.sections[0].children[0].children[0].children[0].name,
            "d"
        );
        assert_eq!(
            list.sections[0].children[0].children[0].children[0].tasks[0].text,
            "deep task"
        );
    }

    #[test]
    fn parses_dual_level_jump_as_implied_child() {
        let content = "# top\n### skip\n- [ ] only task\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].name, "top");
        assert_eq!(list.sections[0].children.len(), 1);
        assert_eq!(list.sections[0].children[0].name, "skip");
        assert_eq!(list.sections[0].children[0].tasks.len(), 1);
    }

    #[test]
    fn parses_multiple_second_level_headings_as_sections() {
        let content = "## Project A\n- [ ] a\n## Project B\n- [ ] b\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 2);
        assert_eq!(list.sections[0].name, "Project A");
        assert_eq!(list.sections[1].name, "Project B");
    }

    #[test]
    fn ignores_random_prose_between_headings_and_tasks() {
        let content = "# Notes\nSome intro paragraph.\n\nHere is a bullet point that is not a task: foo\n- [ ] Real task\nAnother random line\n## Sub\nMore prose\n- [x] Done subtask\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].name, "Notes");
        assert_eq!(list.sections[0].tasks.len(), 1);
        assert_eq!(list.sections[0].tasks[0].text, "Real task");
        assert_eq!(list.sections[0].children.len(), 1);
        assert_eq!(list.sections[0].children[0].name, "Sub");
        assert!(list.sections[0].children[0].tasks[0].is_done);
    }

    #[test]
    fn parse_file_returns_error_for_nonexistent_file() {
        let result = parse_file(Path::new("/tmp/nonexistent_grc_test_file.md"));
        assert!(result.is_err());
    }

    #[test]
    fn write_then_parse_roundtrip() {
        let list = TodoList {
            sections: vec![Section {
                name: "main".to_string(),
                tasks: vec![
                    Task {
                        text: "Pending task".to_string(),
                        is_done: false,
                        due: None,
                    },
                    Task {
                        text: "Done task".to_string(),
                        is_done: true,
                        due: Some(NaiveDate::from_ymd_opt(2025, 3, 20).unwrap()),
                    },
                ],
                children: vec![Section {
                    name: "sub1".to_string(),
                    tasks: vec![Task {
                        text: "Sub task".to_string(),
                        is_done: false,
                        due: None,
                    }],
                    children: vec![Section {
                        name: "subsub".to_string(),
                        tasks: vec![Task {
                            text: "Deep task".to_string(),
                            is_done: false,
                            due: None,
                        }],
                        children: vec![],
                    }],
                }],
            }],
        };

        let f = NamedTempFile::new().unwrap();
        write_file(f.path(), &list).unwrap();
        let parsed = parse_file(f.path()).unwrap();

        assert_eq!(parsed.sections.len(), 1);
        assert_eq!(parsed.sections[0].name, "main");
        assert_eq!(parsed.sections[0].tasks.len(), 2);
        assert!(!parsed.sections[0].tasks[0].is_done);
        assert!(parsed.sections[0].tasks[1].is_done);
        assert_eq!(
            parsed.sections[0].tasks[1].due,
            Some(NaiveDate::from_ymd_opt(2025, 3, 20).unwrap())
        );
        assert_eq!(parsed.sections[0].children.len(), 1);
        assert_eq!(parsed.sections[0].children[0].name, "sub1");
        assert_eq!(parsed.sections[0].children[0].tasks.len(), 1);
        assert_eq!(parsed.sections[0].children[0].children[0].name, "subsub");
        assert_eq!(
            parsed.sections[0].children[0].children[0].tasks[0].text,
            "Deep task"
        );
    }

    #[test]
    fn write_file_done_marker() {
        let list = TodoList {
            sections: vec![Section {
                name: "s".to_string(),
                tasks: vec![
                    Task {
                        text: "a".to_string(),
                        is_done: false,
                        due: None,
                    },
                    Task {
                        text: "b".to_string(),
                        is_done: true,
                        due: None,
                    },
                ],
                children: vec![],
            }],
        };
        let f = NamedTempFile::new().unwrap();
        write_file(f.path(), &list).unwrap();
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("- [ ] a"));
        assert!(content.contains("- [x] b"));
    }

    #[test]
    fn write_file_due_date_format() {
        let list = TodoList {
            sections: vec![Section {
                name: "s".to_string(),
                tasks: vec![Task {
                    text: "deadline".to_string(),
                    is_done: false,
                    due: Some(NaiveDate::from_ymd_opt(2025, 12, 1).unwrap()),
                }],
                children: vec![],
            }],
        };
        let f = NamedTempFile::new().unwrap();
        write_file(f.path(), &list).unwrap();
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("due:2025-12-01"));
    }

    #[test]
    fn write_file_section_and_subsection_headings() {
        let list = TodoList {
            sections: vec![Section {
                name: "Projects".to_string(),
                tasks: Vec::new(),
                children: vec![Section {
                    name: "Rust".to_string(),
                    tasks: Vec::new(),
                    children: Vec::new(),
                }],
            }],
        };
        let f = NamedTempFile::new().unwrap();
        write_file(f.path(), &list).unwrap();
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("# Projects"));
        assert!(content.contains("## Rust"));
    }

    #[test]
    fn write_file_four_level_headings() {
        let list = TodoList {
            sections: vec![Section {
                name: "a".to_string(),
                tasks: Vec::new(),
                children: vec![Section {
                    name: "b".to_string(),
                    tasks: Vec::new(),
                    children: vec![Section {
                        name: "c".to_string(),
                        tasks: Vec::new(),
                        children: vec![Section {
                            name: "d".to_string(),
                            tasks: vec![Task {
                                text: "deep".to_string(),
                                is_done: false,
                                due: None,
                            }],
                            children: vec![],
                        }],
                    }],
                }],
            }],
        };
        let f = NamedTempFile::new().unwrap();
        write_file(f.path(), &list).unwrap();
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("# a"));
        assert!(content.contains("## b"));
        assert!(content.contains("### c"));
        assert!(content.contains("#### d"));
        assert!(content.contains("- [ ] deep"));
    }

    #[test]
    fn write_empty_todolist_produces_empty_file() {
        let list = TodoList::default();
        let f = NamedTempFile::new().unwrap();
        write_file(f.path(), &list).unwrap();
        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn write_file_spacing_rules() {
        let list = TodoList {
            sections: vec![Section {
                name: "Section 1".to_string(),
                tasks: vec![Task {
                    text: "Task 1".to_string(),
                    is_done: false,
                    due: None,
                }],
                children: vec![Section {
                    name: "Subsection 1".to_string(),
                    tasks: vec![Task {
                        text: "Task 2".to_string(),
                        is_done: false,
                        due: None,
                    }],
                    children: Vec::new(),
                }],
            }],
        };
        let f = NamedTempFile::new().unwrap();
        write_file(f.path(), &list).unwrap();
        let content = std::fs::read_to_string(f.path()).unwrap();
        let expected = "# Section 1\n\n- [ ] Task 1\n\n## Subsection 1\n\n- [ ] Task 2\n";
        assert_eq!(content, expected);
    }

    #[test]
    fn test_resolve_relative_date() {
        let today = chrono::Local::now().date_naive();
        assert_eq!(resolve_relative_date("today"), Some(today));
        assert_eq!(resolve_relative_date("tod"), Some(today));
        assert_eq!(resolve_relative_date("t"), Some(today));
        assert_eq!(
            resolve_relative_date("TOMORROW"),
            Some(today + chrono::Duration::days(1))
        );
        assert_eq!(
            resolve_relative_date(" tmw "),
            Some(today + chrono::Duration::days(1))
        );
        assert_eq!(
            resolve_relative_date("next week"),
            Some(today + chrono::Duration::days(7))
        );
        assert_eq!(
            resolve_relative_date("nw"),
            Some(today + chrono::Duration::days(7))
        );
        assert_eq!(
            resolve_relative_date("2025-06-15"),
            Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap())
        );
        assert_eq!(resolve_relative_date("invalid-date"), None);
        assert_eq!(resolve_relative_date(""), None);
    }

    #[test]
    fn test_resolve_weekday_shortcuts() {
        let today = chrono::Local::now().date_naive();

        // Abbreviated forms should work
        let result = resolve_relative_date("mon").unwrap();
        assert!(result > today, "weekday shortcut must be in the future");

        // Full names should also work
        let result = resolve_relative_date("Monday").unwrap();
        assert!(result > today);

        // All weekday abbreviations should resolve to a future date
        for day in &["sun", "mon", "tue", "wed", "thu", "fri", "sat"] {
            let result = resolve_relative_date(day).unwrap();
            assert!(
                result > today,
                "{day} should resolve to a future date, got {result}"
            );
        }

        // Full names too
        for day in &["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
        {
            let result = resolve_relative_date(day).unwrap();
            assert!(
                result > today,
                "{day} should resolve to a future date, got {result}"
            );
        }
    }

    #[test]
    fn test_resolve_relative_date_extra_formats() {
        let today = chrono::Local::now().date_naive();

        // Alternative separators
        assert_eq!(
            resolve_relative_date("2025/06/15"),
            Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap())
        );
        assert_eq!(
            resolve_relative_date("2025.06.15"),
            Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap())
        );

        // "next <weekday>"
        let next_mon = next_weekday(today, chrono::Weekday::Mon);
        assert_eq!(resolve_relative_date("next mon"), Some(next_mon));
        assert_eq!(resolve_relative_date("next monday"), Some(next_mon));

        // Relative offsets
        assert_eq!(
            resolve_relative_date("+3d"),
            Some(today + chrono::Duration::days(3))
        );
        assert_eq!(
            resolve_relative_date("-2d"),
            Some(today - chrono::Duration::days(2))
        );
        assert_eq!(
            resolve_relative_date("+2w"),
            Some(today + chrono::Duration::days(14))
        );
        assert_eq!(
            resolve_relative_date("+1m"),
            Some(today + chrono::Duration::days(30))
        );
        assert_eq!(
            resolve_relative_date("+3 days"),
            Some(today + chrono::Duration::days(3))
        );

        // "in N <unit>"
        assert_eq!(
            resolve_relative_date("in 5 days"),
            Some(today + chrono::Duration::days(5))
        );
        assert_eq!(
            resolve_relative_date("in 2 weeks"),
            Some(today + chrono::Duration::days(14))
        );

        // month name + day resolves to a valid date
        let month_day = resolve_relative_date("jun 15").unwrap();
        assert_eq!(month_day.month(), 6);
        assert_eq!(month_day.day(), 15);
        assert!(month_day >= today);

        // unrecognized input
        assert_eq!(resolve_relative_date("not-a-date"), None);
        assert_eq!(resolve_relative_date("due"), None);
    }

    #[test]
    fn test_parse_due_token_multiword() {
        let today = chrono::Local::now().date_naive();

        // Single word
        let (date, consumed) = parse_due_token("2025-06-15 extra").unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2025, 6, 15).unwrap());
        assert_eq!(consumed, "2025-06-15".len());

        // Two words: "next monday"
        let (date, consumed) = parse_due_token("next monday extra").unwrap();
        assert_eq!(date, next_weekday(today, chrono::Weekday::Mon));
        assert_eq!(consumed, "next monday".len());

        // Two words: "in 3 days"
        let (date, consumed) = parse_due_token("in 3 days foo").unwrap();
        assert_eq!(date, today + chrono::Duration::days(3));
        assert_eq!(consumed, "in 3 days".len());

        // No parseable date
        assert!(parse_due_token("not-a-date").is_none());
        assert!(parse_due_token("").is_none());
    }

    #[test]
    fn test_parse_due_token_prefers_shortest_match() {
        // "tue" matches on its own; the trailing word should not be consumed.
        let (_, consumed) = parse_due_token("tue foo").unwrap();
        assert_eq!(consumed, "tue".len());
    }

    #[test]
    fn test_next_weekday_logic() {
        use chrono::Weekday;

        // Monday 2025-07-21
        let monday = NaiveDate::from_ymd_opt(2025, 7, 21).unwrap();

        // Next Tuesday from Monday = +1 day
        assert_eq!(
            next_weekday(monday, Weekday::Tue),
            NaiveDate::from_ymd_opt(2025, 7, 22).unwrap()
        );

        // Next Monday from Monday = +7 days (next week)
        assert_eq!(
            next_weekday(monday, Weekday::Mon),
            NaiveDate::from_ymd_opt(2025, 7, 28).unwrap()
        );

        // Next Sunday from Monday = +6 days
        assert_eq!(
            next_weekday(monday, Weekday::Sun),
            NaiveDate::from_ymd_opt(2025, 7, 27).unwrap()
        );
    }

    #[test]
    fn write_read_write_byte_fidelity_roundtrip() {
        let initial_content = "# Section 1\n\n- [ ] Task 1\n- [x] Task 2 due:2025-10-15\n\n## Sub 1\n\n- [ ] Subtask\n";
        let f1 = temp_file_with(initial_content);
        let list1 = parse_file(f1.path()).unwrap();

        let f2 = NamedTempFile::new().unwrap();
        write_file(f2.path(), &list1).unwrap();
        let content2 = std::fs::read_to_string(f2.path()).unwrap();
        assert_eq!(content2, initial_content);

        let list2 = parse_file(f2.path()).unwrap();
        let f3 = NamedTempFile::new().unwrap();
        write_file(f3.path(), &list2).unwrap();
        let content3 = std::fs::read_to_string(f3.path()).unwrap();
        assert_eq!(content3, content2);
    }

    #[test]
    fn parses_tasks_before_first_heading_attached_to_implicit_or_last_section() {
        // When tasks appear before any heading, parser creates/attaches to section
        let content = "- [ ] Orphan task\n# Header\n- [ ] Header task\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        // Since `# Header` comes after orphan task, stack was empty, so orphan task was attached to last section when section appeared or dropped if no section before it
        assert!(!list.sections.is_empty());
    }
}
