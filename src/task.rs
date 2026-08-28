use chrono::NaiveDate;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub text: String,
    pub is_done: bool,
    pub due: Option<NaiveDate>,
}

impl Task {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_done: false,
            due: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub tasks: Vec<Task>,
    pub children: Vec<Self>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TodoList {
    pub sections: Vec<Section>,
}

impl Section {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn count_tasks(&self) -> usize {
        self.tasks.len() + self.children.iter().map(Self::count_tasks).sum::<usize>()
    }
}

pub type NodePath = Vec<usize>;

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
    fn task_creation_new() {
        let task = Task::new("Buy milk");
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
    fn section_creation() {
        let sec = Section::new("work");
        assert_eq!(sec.name, "work");
        assert!(sec.tasks.is_empty());
        assert!(sec.children.is_empty());
    }

    #[test]
    fn nested_section_hierarchy() {
        let mut root = Section::new("work");
        root.tasks.push(Task::new("Top-level"));
        root.children.push(Section::new("backend"));
        root.children[0].children.push(Section::new("api"));
        root.children[0].children[0]
            .tasks
            .push(Task::new("Fix endpoint"));

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
}
