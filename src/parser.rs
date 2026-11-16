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
