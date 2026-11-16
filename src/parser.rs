//! Markdown parsing and serialization.
//!
//! The parser is deliberately small and line-oriented. Structural lines are
//! headings (`#`, `##`, ...) and checkbox tasks (`- [ ]`, `- [x]`). Every other
//! line is retained as raw text attached to the next structural item, so a save
//! never silently deletes prose, comments, or blank lines.
//!
//! Parsing and serialization are separate from the TUI and can be tested with
//! plain strings.

use crate::due::extract_inline_due;
use crate::task::{LineEnding, NodePath, Section, Task, TodoList, get_node_mut};
use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Read and parse a Markdown file.
pub fn parse_file(path: &Path) -> Result<TodoList> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read todo file: {}", path.display()))?;
    Ok(parse_str(&content))
}

/// Parse Markdown content into a [`TodoList`].
pub fn parse_str(content: &str) -> TodoList {
    let mut list = TodoList {
        line_ending: detect_line_ending(content),
        final_newline: content.ends_with('\n'),
        ..TodoList::default()
    };

    let mut stack: Vec<(usize, NodePath)> = Vec::new();
    let mut pending_raw: Vec<String> = Vec::new();

    for line in logical_lines(content) {
        let trimmed = line.trim();

        if let Some((level, name)) = parse_heading(trimmed) {
            while stack
                .last()
                .is_some_and(|(stack_level, _)| *stack_level >= level)
            {
                stack.pop();
            }

            let parent_path = stack.last().map(|(_, path)| path.clone());
            let mut section = Section::with_level(name, level);
            section.leading_raw = Some(std::mem::take(&mut pending_raw));

            let path = match &parent_path {
                None => {
                    list.sections.push(section);
                    vec![list.sections.len() - 1]
                }
                Some(parent) => {
                    let Some(target) = get_node_mut(&mut list, parent) else {
                        // The parser only builds paths it has just pushed, so this
                        // is unreachable in practice. Preserve the heading as
                        // prose rather than dropping it.
                        pending_raw.extend(section.leading_raw.take().unwrap_or_default());
                        pending_raw.push(line.to_string());
                        continue;
                    };
                    let index = target.children.len();
                    target.children.push(section);
                    let mut path = parent.clone();
                    path.push(index);
                    path
                }
            };

            stack.push((level, path));
            continue;
        }

        if let Some((is_done, text)) = parse_task(trimmed) {
            let (text, due) = extract_inline_due(text);
            let mut task = Task::new(text);
            task.is_done = is_done;
            task.due = due;
            task.leading_raw = Some(std::mem::take(&mut pending_raw));

            match stack.last() {
                Some((_, path)) => {
                    if let Some(section) = get_node_mut(&mut list, path) {
                        section.tasks.push(task);
                    } else {
                        list.unsectioned_tasks.push(task);
                    }
                }
                None => list.unsectioned_tasks.push(task),
            }
            continue;
        }

        pending_raw.push(line.to_string());
    }

    list.trailing_raw = pending_raw;
    list
}

/// Serialize a [`TodoList`] to Markdown.
pub fn to_markdown(list: &TodoList) -> String {
    let mut writer = MarkdownWriter {
        lines: Vec::new(),
        last: LastLine::Nothing,
    };

    for task in &list.unsectioned_tasks {
        writer.task(task);
    }
    for section in &list.sections {
        writer.section(section, 0);
    }
    writer.lines.extend(list.trailing_raw.iter().cloned());

    let mut output = writer.lines.join(list.line_ending.as_str());
    if list.final_newline && !writer.lines.is_empty() {
        output.push_str(list.line_ending.as_str());
    }
    output
}

/// Atomically write a todo list to `path`.
///
/// The content is written to a uniquely named temporary file in the same
/// directory, flushed, and renamed over the destination. A failure removes the
/// temporary file and leaves the original untouched.
pub fn write_file(path: &Path, list: &TodoList) -> Result<()> {
    let content = to_markdown(list);
    atomic_write(path, content.as_bytes())
        .with_context(|| format!("Failed to write todo file: {}", path.display()))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary = unique_temp_path(directory);

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);

        // Keep the destination's permissions when replacing an existing file.
        if let Ok(metadata) = fs::metadata(path) {
            let _ = fs::set_permissions(&temporary, metadata.permissions());
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn unique_temp_path(directory: &Path) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    directory.join(format!(".grc_{}_{}.tmp", std::process::id(), nanos))
}

fn detect_line_ending(content: &str) -> LineEnding {
    if content.contains("\r\n") {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    }
}

/// Split content into lines while preserving whether the final line ended.
fn logical_lines(content: &str) -> impl Iterator<Item = &str> {
    content.split_inclusive('\n').map(|segment| {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        line.strip_suffix('\r').unwrap_or(line)
    })
}

/// Parse a heading line, returning its level and name.
fn parse_heading(trimmed: &str) -> Option<(usize, String)> {
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if hashes == 0 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(' ') {
        return None;
    }
    let name = rest.trim();
    (!name.is_empty()).then(|| (hashes, name.to_string()))
}

/// Parse a checkbox task line.
///
/// A separator is required after the closing bracket, so `- [ ]foo` remains
/// ordinary prose rather than becoming a task.
fn parse_task(trimmed: &str) -> Option<(bool, &str)> {
    const CHECKBOXES: [(&str, bool); 3] = [("- [ ]", false), ("- [x]", true), ("- [X]", true)];

    CHECKBOXES.iter().find_map(|(prefix, done)| {
        let rest = trimmed.strip_prefix(prefix)?;
        if rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace) {
            Some((*done, rest.trim()))
        } else {
            None
        }
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LastLine {
    Nothing,
    Heading,
    Task,
    Raw,
}

struct MarkdownWriter {
    lines: Vec<String>,
    last: LastLine,
}

impl MarkdownWriter {
    fn section(&mut self, section: &Section, parent_level: usize) {
        let level = if parent_level == 0 {
            section.level.max(1)
        } else {
            section.level.max(parent_level + 1)
        };

        match &section.leading_raw {
            Some(raw) => self.raw(raw),
            None if self.last == LastLine::Task => self.blank(),
            None => {}
        }

        self.line(format!("{} {}", "#".repeat(level), section.name));
        self.last = LastLine::Heading;

        for (index, task) in section.tasks.iter().enumerate() {
            if task.leading_raw.is_none() && index == 0 && self.last == LastLine::Heading {
                self.blank();
            }
            self.task(task);
        }

        for child in &section.children {
            self.section(child, level);
        }
    }

    fn task(&mut self, task: &Task) {
        if let Some(raw) = &task.leading_raw {
            self.raw(raw);
        }

        let checkbox = if task.is_done { "[x]" } else { "[ ]" };
        match task.due {
            Some(date) => self.line(format!(
                "- {checkbox} {} due:{}",
                task.text,
                date.format("%Y-%m-%d")
            )),
            None => self.line(format!("- {checkbox} {}", task.text)),
        }
        self.last = LastLine::Task;
    }

    fn raw(&mut self, raw: &[String]) {
        for line in raw {
            self.line(line.clone());
        }
        if !raw.is_empty() {
            self.last = LastLine::Raw;
        }
    }

    fn blank(&mut self) {
        self.line(String::new());
    }

    fn line(&mut self, line: String) {
        self.lines.push(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::get_node;
    use chrono::NaiveDate;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn roundtrip(input: &str) -> (TodoList, String) {
        let list = parse_str(input);
        let output = to_markdown(&list);
        (list, output)
    }

    #[test]
    fn parses_single_section_with_tasks() {
        let list = parse_str("# main\n- [ ] Buy milk\n- [x] Walk dog\n");
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].name, "main");
        assert_eq!(list.sections[0].level, 1);
        assert_eq!(list.sections[0].tasks.len(), 2);
        assert!(!list.sections[0].tasks[0].is_done);
        assert_eq!(list.sections[0].tasks[0].text, "Buy milk");
        assert!(list.sections[0].tasks[1].is_done);
    }

    #[test]
    fn parses_uppercase_done_marker() {
        let list = parse_str("# sec\n- [X] Done task\n");
        assert!(list.sections[0].tasks[0].is_done);
    }

    #[test]
    fn malformed_checkboxes_are_prose() {
        let list = parse_str("# sec\n- [ ]not a task\n- [x]nope\n- [~] no\n");
        assert!(list.sections[0].tasks.is_empty());
        assert_eq!(list.trailing_raw.len(), 3);
    }

    #[test]
    fn parses_due_date_and_strips_token() {
        let list = parse_str("# sec\n- [ ] Submit report due:2025-01-15\n");
        let task = &list.sections[0].tasks[0];
        assert_eq!(task.text, "Submit report");
        assert_eq!(task.due, Some(date(2025, 1, 15)));
    }

    #[test]
    fn due_in_middle_of_text_keeps_spacing() {
        let list = parse_str("# sec\n- [ ] Submit due:2025-01-15 report\n");
        let task = &list.sections[0].tasks[0];
        assert_eq!(task.text, "Submit report");
        assert_eq!(task.due, Some(date(2025, 1, 15)));
    }

    #[test]
    fn due_token_is_not_eaten_inside_words_or_urls() {
        let list =
            parse_str("# sec\n- [ ] overdue:2025-01-01\n- [ ] https://example.com/due:today\n");
        assert_eq!(list.sections[0].tasks[0].text, "overdue:2025-01-01");
        assert_eq!(list.sections[0].tasks[0].due, None);
        assert_eq!(
            list.sections[0].tasks[1].text,
            "https://example.com/due:today"
        );
        assert_eq!(list.sections[0].tasks[1].due, None);
    }

    #[test]
    fn parsed_unknown_lines_are_kept_verbatim() {
        let input = "\
Intro prose

<!-- a comment -->
# work

Some notes
- [ ] Task A

A paragraph after task A
## backend

- [ ] Task B
Trailing text
";
        let (list, output) = roundtrip(input);
        assert_eq!(output, input);
        assert_eq!(list.sections[0].name, "work");
        assert_eq!(list.sections[0].tasks[0].text, "Task A");
        assert_eq!(list.sections[0].children[0].name, "backend");
    }

    #[test]
    fn blank_lines_are_preserved() {
        let input = "# a\n\n\n\n- [ ] t\n\n\n# b\n";
        let (_, output) = roundtrip(input);
        assert_eq!(output, input);
    }

    #[test]
    fn heading_levels_are_preserved() {
        let input = "# top\n### skipped\n#### deeper\n## sibling\n";
        let (list, output) = roundtrip(input);
        assert_eq!(output, input);
        assert_eq!(list.sections[0].children[0].level, 3);
        assert_eq!(list.sections[0].children[0].children[0].level, 4);
        assert_eq!(list.sections[0].children[1].level, 2);
    }

    #[test]
    fn nested_subsections_attach_to_correct_parents() {
        let list =
            parse_str("# work\n- [ ] Top\n## backend\n- [ ] Fix API\n## frontend\n- [x] Style\n");
        assert_eq!(list.sections.len(), 1);
        assert_eq!(list.sections[0].tasks.len(), 1);
        assert_eq!(list.sections[0].children.len(), 2);
        assert_eq!(list.sections[0].children[0].name, "backend");
        assert_eq!(list.sections[0].children[1].name, "frontend");
        assert!(list.sections[0].children[1].tasks[0].is_done);
    }

    #[test]
    fn multiple_top_level_sections() {
        let list = parse_str("# work\n- [ ] A\n# personal\n- [ ] B\n");
        assert_eq!(list.sections.len(), 2);
        assert_eq!(list.sections[0].name, "work");
        assert_eq!(list.sections[1].name, "personal");
    }

    #[test]
    fn empty_file_is_empty_list() {
        let list = parse_str("");
        assert!(list.sections.is_empty());
        assert!(list.unsectioned_tasks.is_empty());
        assert_eq!(to_markdown(&list), "");
    }

    #[test]
    fn headings_only_are_preserved() {
        let input = "# empty\n## sub\n";
        let (list, output) = roundtrip(input);
        assert_eq!(output, input);
        assert!(list.sections[0].tasks.is_empty());
        assert_eq!(list.sections[0].children.len(), 1);
    }

    #[test]
    fn skipped_heading_levels_become_nested() {
        let list = parse_str("# top\n### skip\n- [ ] only\n");
        assert_eq!(list.sections[0].children[0].name, "skip");
        assert_eq!(list.sections[0].children[0].level, 3);
        assert_eq!(list.sections[0].children[0].tasks.len(), 1);
    }

    #[test]
    fn deep_heading_hierarchy() {
        let list = parse_str("# a\n## b\n### c\n#### d\n- [ ] deep\n");
        let d = get_node(&list, &[0, 0, 0, 0]).unwrap();
        assert_eq!(d.name, "d");
        assert_eq!(d.tasks[0].text, "deep");
    }

    #[test]
    fn tasks_before_first_heading_are_preserved_but_unsectioned() {
        let input = "pre\n- [ ] Orphan\n# Header\n- [ ] Header task\n";
        let (list, output) = roundtrip(input);
        assert_eq!(output, input);
        assert_eq!(list.unsectioned_tasks.len(), 1);
        assert_eq!(list.unsectioned_tasks[0].text, "Orphan");
        assert_eq!(list.sections[0].tasks.len(), 1);
    }

    #[test]
    fn prose_only_file_roundtrips() {
        let input = "Just some notes.\n\n- not a task\n";
        let (list, output) = roundtrip(input);
        assert_eq!(output, input);
        assert!(list.sections.is_empty());
        assert_eq!(list.trailing_raw.len(), 3);
    }

    #[test]
    fn crlf_is_preserved() {
        let input = "# sec\r\n\r\n- [ ] Task\r\n";
        let (list, output) = roundtrip(input);
        assert_eq!(list.line_ending, LineEnding::Crlf);
        assert_eq!(output, input);
    }

    #[test]
    fn missing_final_newline_is_preserved() {
        let input = "# sec\n- [ ] Task";
        let (list, output) = roundtrip(input);
        assert!(!list.final_newline);
        assert_eq!(output, input);
    }

    #[test]
    fn very_long_lines_survive() {
        let long = "x".repeat(100_000);
        let input = format!("# sec\n- [ ] {long}\n");
        let (list, output) = roundtrip(&input);
        assert_eq!(output, input);
        assert_eq!(list.sections[0].tasks[0].text.len(), 100_000);
    }

    #[test]
    fn write_read_write_preserves_model_and_bytes() {
        let input = "# Section 1\n\n- [ ] Task 1\n- [x] Task 2 due:2025-10-15\n\n## Sub 1\n\n- [ ] Subtask\n";
        let first = parse_str(input);
        let first_output = to_markdown(&first);
        assert_eq!(first_output, input);

        let second = parse_str(&first_output);
        assert_eq!(first, second);
        assert_eq!(to_markdown(&second), first_output);
    }

    #[test]
    fn programmatic_document_gets_canonical_spacing() {
        let mut list = TodoList::default();
        let mut section = Section::new("Section 1");
        section.tasks.push(Task::new("Task 1"));
        let mut child = Section::new("Subsection 1");
        child.tasks.push(Task::new("Task 2"));
        section.children.push(child);
        list.sections.push(section);

        assert_eq!(
            to_markdown(&list),
            "# Section 1\n\n- [ ] Task 1\n\n## Subsection 1\n\n- [ ] Task 2\n"
        );
    }

    #[test]
    fn programmatic_due_and_done_markers() {
        let mut list = TodoList::default();
        let mut section = Section::new("s");
        let mut done = Task::new("b");
        done.is_done = true;
        let mut due = Task::new("deadline");
        due.due = Some(date(2025, 12, 1));
        section.tasks.extend([Task::new("a"), done, due]);
        list.sections.push(section);

        let markdown = to_markdown(&list);
        assert!(markdown.contains("- [ ] a"));
        assert!(markdown.contains("- [x] b"));
        assert!(markdown.contains("- [ ] deadline due:2025-12-01"));
    }

    #[test]
    fn file_io_roundtrip_and_missing_file_error() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("todo.md");
        let input = "# sec\n\n- [ ] Task\n";
        fs::write(&path, input).unwrap();

        let list = parse_file(&path).unwrap();
        write_file(&path, &list).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), input);

        let missing = directory.path().join("missing.md");
        assert!(parse_file(&missing).is_err());
    }

    #[test]
    fn file_write_preserves_crlf_and_missing_final_newline() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("todo.md");
        let input = "# sec\r\n\r\n- [ ] Task";
        fs::write(&path, input).unwrap();

        let list = parse_file(&path).unwrap();
        write_file(&path, &list).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), input);
    }

    #[test]
    fn temp_file_is_removed_on_success() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("todo.md");
        let list = TodoList::default();
        write_file(&path, &list).unwrap();
        let leftovers = fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".grc_"))
            .count();
        assert_eq!(leftovers, 0);
    }
}
