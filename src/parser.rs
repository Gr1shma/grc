use crate::task::{Section, Subsection, Task, TodoList};
use anyhow::Result;
use chrono::NaiveDate;
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
        _ => NaiveDate::parse_from_str(&cleaned, "%Y-%m-%d").ok(),
    }
}

pub fn parse_file(path: &Path) -> Result<TodoList> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut todo_list = TodoList::default();

    let mut current_section: Option<Section> = None;
    let mut current_subsection: Option<Subsection> = None;

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.starts_with("## ") {
            if let Some(sub) = current_subsection.take()
                && let Some(ref mut sec) = current_section
            {
                sec.subsections.push(sub);
            }
            let sub_name = trimmed.trim_start_matches("## ").trim().to_string();
            current_subsection = Some(Subsection {
                name: sub_name,
                tasks: Vec::new(),
            });
        } else if trimmed.starts_with("# ") {
            if let Some(sub) = current_subsection.take()
                && let Some(ref mut sec) = current_section
            {
                sec.subsections.push(sub);
            }
            if let Some(sec) = current_section.take() {
                todo_list.sections.push(sec);
            }
            let sec_name = trimmed.trim_start_matches("# ").trim().to_string();
            current_section = Some(Section {
                name: sec_name,
                tasks: Vec::new(),
                subsections: Vec::new(),
            });
        } else if trimmed.starts_with("- [ ]")
            || trimmed.starts_with("- [x]")
            || trimmed.starts_with("- [X]")
        {
            let is_done = trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]");
            let mut task_text = trimmed[5..].trim().to_string();

            let mut due = None;
            if let Some(idx) = task_text.find("due:") {
                let date_str = task_text[idx + 4..].split_whitespace().next().unwrap_or("");
                if let Ok(parsed_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    due = Some(parsed_date);
                    task_text = task_text
                        .replace(&format!("due:{}", date_str), "")
                        .trim()
                        .to_string();
                }
            }

            let task = Task {
                text: task_text,
                is_done,
                due,
            };

            if let Some(ref mut sub) = current_subsection {
                sub.tasks.push(task);
            } else if let Some(ref mut sec) = current_section {
                sec.tasks.push(task);
            }
        }
    }

    if let Some(sub) = current_subsection
        && let Some(ref mut sec) = current_section
    {
        sec.subsections.push(sub);
    }
    if let Some(sec) = current_section {
        todo_list.sections.push(sec);
    }

    Ok(todo_list)
}

pub fn write_file(path: &Path, todo_list: &TodoList) -> Result<()> {
    let mut file = File::create(path)?;
    let mut first = true;
    let mut last_wrote_tasks = false;

    for sec in &todo_list.sections {
        if !first && last_wrote_tasks {
            writeln!(file)?;
        }
        writeln!(file, "# {}", sec.name)?;
        first = false;

        if !sec.tasks.is_empty() {
            writeln!(file)?;
            for task in &sec.tasks {
                write_task_line(&mut file, task)?;
            }
            last_wrote_tasks = true;
        } else {
            last_wrote_tasks = false;
        }

        for sub in &sec.subsections {
            if !first && last_wrote_tasks {
                writeln!(file)?;
            }
            writeln!(file, "## {}", sub.name)?;
            first = false;

            if !sub.tasks.is_empty() {
                writeln!(file)?;
                for task in &sub.tasks {
                    write_task_line(&mut file, task)?;
                }
                last_wrote_tasks = true;
            } else {
                last_wrote_tasks = false;
            }
        }
    }
    Ok(())
}

fn write_task_line(file: &mut File, task: &Task) -> Result<()> {
    let box_str = if task.is_done { "[x]" } else { "[ ]" };
    if let Some(date) = task.due {
        writeln!(
            file,
            "- {} {} due:{}",
            box_str,
            task.text,
            date.format("%Y-%m-%d")
        )?;
    } else {
        writeln!(file, "- {} {}", box_str, task.text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Section, Subsection, Task, TodoList};
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
    fn parses_subsections() {
        let content =
            "# work\n- [ ] Top task\n## backend\n- [ ] Fix API\n## frontend\n- [x] Style button\n";
        let f = temp_file_with(content);
        let list = parse_file(f.path()).unwrap();
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].tasks.len(), 1);
        assert_eq!(list.sections[0].subsections.len(), 2);
        assert_eq!(list.sections[0].subsections[0].name, "backend");
        assert_eq!(list.sections[0].subsections[1].name, "frontend");
        assert!(list.sections[0].subsections[1].tasks[0].is_done);
    }

    #[test]
    fn parses_multiple_sections() {
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
        assert_eq!(list.sections[0].subsections.len(), 1);
        assert!(list.sections[0].subsections[0].tasks.is_empty());
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
                subsections: vec![Subsection {
                    name: "sub1".to_string(),
                    tasks: vec![Task {
                        text: "Sub task".to_string(),
                        is_done: false,
                        due: None,
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
        assert_eq!(parsed.sections[0].subsections.len(), 1);
        assert_eq!(parsed.sections[0].subsections[0].name, "sub1");
        assert_eq!(parsed.sections[0].subsections[0].tasks.len(), 1);
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
                subsections: Vec::new(),
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
                subsections: Vec::new(),
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
                subsections: vec![Subsection {
                    name: "Rust".to_string(),
                    tasks: Vec::new(),
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
                subsections: vec![Subsection {
                    name: "Subsection 1".to_string(),
                    tasks: vec![Task {
                        text: "Task 2".to_string(),
                        is_done: false,
                        due: None,
                    }],
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
}
