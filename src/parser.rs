use crate::task::{Section, Task, TodoList, get_node_mut};
use anyhow::Result;
use chrono::{Datelike, NaiveDate, Weekday};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

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
        _ => NaiveDate::parse_from_str(&cleaned, "%Y-%m-%d").ok(),
    }
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
                let date_str = task_text[idx + 4..].split_whitespace().next().unwrap_or("");
                // Try absolute date first, then relative date shortcuts
                let parsed_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .ok()
                    .or_else(|| resolve_relative_date(date_str));
                if let Some(parsed_date) = parsed_date {
                    due = Some(parsed_date);
                    let due_token_len = 4 + date_str.len();
                    task_text = format!(
                        "{}{}",
                        task_text[..idx].trim_end(),
                        task_text[idx + due_token_len..].trim_start()
                    )
                    .trim()
                    .to_string();
                }
            }

            let task = Task {
                text: task_text,
                is_done,
                due,
            };

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
    use std::time::{SystemTime, UNIX_EPOCH};
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
}
