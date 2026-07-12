use chrono::NaiveDate;

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub text: String,
    pub is_done: bool,
    pub due: Option<NaiveDate>,
}

/// A heading in the markdown file. Headings can be nested arbitrarily deep:
/// each section owns its direct tasks and a list of child sections.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub tasks: Vec<Task>,
    pub children: Vec<Section>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TodoList {
    pub sections: Vec<Section>,
}

impl Section {
    pub fn new(name: impl Into<String>) -> Self {
        Section {
            name: name.into(),
            tasks: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Total number of tasks in this section and all of its descendants.
    pub fn count_tasks(&self) -> usize {
        let mut n = self.tasks.len();
        for child in &self.children {
            n += child.count_tasks();
        }
        n
    }
}

/// A path of child indices identifying a section in the tree, e.g. `[2, 0]`
/// means the first child of the third top-level section.
pub type NodePath = Vec<usize>;

/// Immutably borrow the section located at `path`.
pub fn get_node<'a>(list: &'a TodoList, path: &[usize]) -> Option<&'a Section> {
    get_node_slice(&list.sections, path)
}

fn get_node_slice<'a>(children: &'a [Section], path: &[usize]) -> Option<&'a Section> {
    if path.is_empty() {
        return None;
    }
    let sec = children.get(path[0])?;
    get_node_slice_impl(sec, &path[1..])
}

fn get_node_slice_impl<'a>(sec: &'a Section, rest: &[usize]) -> Option<&'a Section> {
    if rest.is_empty() {
        return Some(sec);
    }
    get_node_slice_impl(sec.children.get(rest[0])?, &rest[1..])
}

/// Mutably borrow the section located at `path`.
pub fn get_node_mut<'a>(list: &'a mut TodoList, path: &[usize]) -> Option<&'a mut Section> {
    if path.is_empty() {
        return None;
    }
    let sec = list.sections.get_mut(path[0])?;
    get_node_mut_impl(sec, &path[1..])
}

fn get_node_mut_impl<'a>(sec: &'a mut Section, rest: &[usize]) -> Option<&'a mut Section> {
    if rest.is_empty() {
        return Some(sec);
    }
    get_node_mut_impl(sec.children.get_mut(rest[0])?, &rest[1..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn task_creation_pending_no_due() {
        let task = Task {
            text: "Buy milk".to_string(),
            is_done: false,
            due: None,
        };
        assert_eq!(task.text, "Buy milk");
        assert!(!task.is_done);
        assert!(task.due.is_none());
    }

    #[test]
    fn task_creation_done_with_due() {
        let due = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let task = Task {
            text: "Submit report".to_string(),
            is_done: true,
            due: Some(due),
        };
        assert!(task.is_done);
        assert_eq!(task.due.unwrap(), due);
    }

    #[test]
    fn task_clone_is_independent() {
        let original = Task {
            text: "Original".to_string(),
            is_done: false,
            due: None,
        };
        let mut cloned = original.clone();
        cloned.text = "Cloned".to_string();
        cloned.is_done = true;
        assert_eq!(original.text, "Original");
        assert!(!original.is_done);
        assert_eq!(cloned.text, "Cloned");
        assert!(cloned.is_done);
    }

    #[test]
    fn section_creation() {
        let sec = Section::new("work");
        assert_eq!(sec.name, "work");
        assert!(sec.tasks.is_empty());
        assert!(sec.children.is_empty());
    }

    #[test]
    fn nested_section_hierarchy() {
        let mut root = Section::new("work");
        root.tasks.push(Task {
            text: "Top-level".to_string(),
            is_done: false,
            due: None,
        });
        root.children.push(Section::new("backend"));
        root.children[0].children.push(Section::new("api"));
        root.children[0].children[0].tasks.push(Task {
            text: "Fix endpoint".to_string(),
            is_done: false,
            due: None,
        });

        assert_eq!(root.name, "work");
        assert_eq!(root.tasks.len(), 1);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "backend");
        assert_eq!(root.children[0].children[0].name, "api");
        assert_eq!(root.children[0].children[0].tasks[0].text, "Fix endpoint");
    }

    #[test]
    fn todolist_default_is_empty() {
        let list = TodoList::default();
        assert!(list.sections.is_empty());
    }

    #[test]
    fn todolist_clone_is_independent() {
        let mut original = TodoList::default();
        original.sections.push(Section::new("main"));
        let cloned = original.clone();
        original.sections.clear();
        assert!(original.sections.is_empty());
        assert_eq!(cloned.sections.len(), 1);
    }
}
