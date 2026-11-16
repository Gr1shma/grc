//! Pure document model.
//!
//! `Task`, `Section`, and `TodoList` describe the Markdown document in memory.
//! This module has no file-system, terminal, or clock dependencies, so tree
//! operations can be unit-tested directly.

use chrono::NaiveDate;

/// Line ending used when the document is serialized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix-style `\n`.
    #[default]
    Lf,
    /// Windows-style `\r\n`.
    Crlf,
}

impl LineEnding {
    /// The literal characters for this line ending.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}

/// A single checkbox task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// Task text without the checkbox or inline due marker.
    pub text: String,
    /// Whether the checkbox is checked.
    pub is_done: bool,
    /// Optional due date.
    pub due: Option<NaiveDate>,
    /// Raw lines (comments, blank lines, prose) that appeared before this task.
    ///
    /// `None` marks a programmatically created task, for which the writer may
    /// generate conventional spacing. `Some` preserves the parsed document.
    pub leading_raw: Option<Vec<String>>,
}

impl Task {
    /// Create a new, incomplete task without a due date.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_done: false,
            due: None,
            leading_raw: None,
        }
    }
}

/// A Markdown heading and everything nested beneath it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Heading text without the leading `#` markers.
    pub name: String,
    /// Heading level, where `#` is 1.
    pub level: usize,
    /// Direct tasks under this heading.
    pub tasks: Vec<Task>,
    /// Nested headings.
    pub children: Vec<Section>,
    /// Raw lines that appeared before this heading.
    pub leading_raw: Option<Vec<String>>,
}

impl Section {
    /// Create a top-level section.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            level: 1,
            tasks: Vec::new(),
            children: Vec::new(),
            leading_raw: None,
        }
    }

    /// Create a section with an explicit heading level.
    pub fn with_level(name: impl Into<String>, level: usize) -> Self {
        let mut section = Self::new(name);
        section.level = level.max(1);
        section
    }

    /// Count tasks in this section and all descendants.
    pub fn count_tasks(&self) -> usize {
        self.tasks.len() + self.children.iter().map(Self::count_tasks).sum::<usize>()
    }
}

/// Root of a parsed todo document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TodoList {
    /// Tasks that appeared before the first heading.
    ///
    /// They are preserved on write but are not currently shown in the TUI,
    /// matching the previous visible behaviour while avoiding data loss.
    pub unsectioned_tasks: Vec<Task>,
    /// Top-level headings.
    pub sections: Vec<Section>,
    /// Raw lines that appeared after the last structural item.
    pub trailing_raw: Vec<String>,
    /// Line ending to use when serializing.
    pub line_ending: LineEnding,
    /// Whether the original file ended with a newline.
    pub final_newline: bool,
}

impl Default for TodoList {
    fn default() -> Self {
        Self {
            unsectioned_tasks: Vec::new(),
            sections: Vec::new(),
            trailing_raw: Vec::new(),
            line_ending: LineEnding::Lf,
            final_newline: true,
        }
    }
}

/// Index path from the document root to a section.
pub type NodePath = Vec<usize>;

/// Look up a section by path.
pub fn get_node<'a>(list: &'a TodoList, path: &[usize]) -> Option<&'a Section> {
    let (first, rest) = path.split_first()?;
    get_child(list.sections.get(*first)?, rest)
}

fn get_child<'a>(section: &'a Section, rest: &[usize]) -> Option<&'a Section> {
    match rest.split_first() {
        None => Some(section),
        Some((index, tail)) => get_child(section.children.get(*index)?, tail),
    }
}

/// Look up a section mutably by path.
pub fn get_node_mut<'a>(list: &'a mut TodoList, path: &[usize]) -> Option<&'a mut Section> {
    let (first, rest) = path.split_first()?;
    get_child_mut(list.sections.get_mut(*first)?, rest)
}

fn get_child_mut<'a>(section: &'a mut Section, rest: &[usize]) -> Option<&'a mut Section> {
    match rest.split_first() {
        None => Some(section),
        Some((index, tail)) => get_child_mut(section.children.get_mut(*index)?, tail),
    }
}

/// Insert a section under `parent` at `index`, forcing its root to `level`.
///
/// Descendant levels are shifted by the same delta, preserving relative
/// heading skips. Returns the new path, or `None` if the parent is missing.
pub fn insert_section(
    list: &mut TodoList,
    parent: Option<&NodePath>,
    index: usize,
    mut section: Section,
    level: usize,
) -> Option<NodePath> {
    let level = level.max(1);
    let delta = level as isize - section.level as isize;
    shift_levels(&mut section, delta);

    match parent {
        None => {
            let index = index.min(list.sections.len());
            list.sections.insert(index, section);
            Some(vec![index])
        }
        Some(parent) => {
            let target = get_node_mut(list, parent)?;
            let index = index.min(target.children.len());
            target.children.insert(index, section);
            let mut path = parent.clone();
            path.push(index);
            Some(path)
        }
    }
}

fn shift_levels(section: &mut Section, delta: isize) {
    section.level = (section.level as isize + delta).max(1) as usize;
    for child in &mut section.children {
        shift_levels(child, delta);
    }
}

/// Remove a section by path, returning it for the clipboard.
pub fn remove_section(list: &mut TodoList, path: &[usize]) -> Option<Section> {
    let (last, parent_path) = path.split_last()?;
    if parent_path.is_empty() {
        return (*last < list.sections.len()).then(|| list.sections.remove(*last));
    }
    let parent = get_node_mut(list, parent_path)?;
    if *last < parent.children.len() {
        Some(parent.children.remove(*last))
    } else {
        None
    }
}

/// Number of direct children at a location (`None` means the document root).
pub fn child_count(list: &TodoList, path: Option<&NodePath>) -> usize {
    path.map_or(list.sections.len(), |path| {
        get_node(list, path).map_or(0, |section| section.children.len())
    })
}

/// Number of direct tasks in a section.
pub fn task_count(list: &TodoList, path: &NodePath) -> usize {
    get_node(list, path).map_or(0, |section| section.tasks.len())
}
