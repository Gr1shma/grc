use chrono::NaiveDate;

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub text: String,
    pub is_done: bool,
    pub due: Option<NaiveDate>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Subsection {
    pub name: String,
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub tasks: Vec<Task>,
    pub subsections: Vec<Subsection>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TodoList {
    pub sections: Vec<Section>,
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
    fn subsection_creation() {
        let sub = Subsection {
            name: "urgent".to_string(),
            tasks: vec![
                Task {
                    text: "Task A".to_string(),
                    is_done: false,
                    due: None,
                },
                Task {
                    text: "Task B".to_string(),
                    is_done: true,
                    due: None,
                },
            ],
        };
        assert_eq!(sub.name, "urgent");
        assert_eq!(sub.tasks.len(), 2);
        assert!(!sub.tasks[0].is_done);
        assert!(sub.tasks[1].is_done);
    }

    #[test]
    fn section_with_tasks_and_subsections() {
        let sec = Section {
            name: "work".to_string(),
            tasks: vec![Task {
                text: "Top-level".to_string(),
                is_done: false,
                due: None,
            }],
            subsections: vec![Subsection {
                name: "backend".to_string(),
                tasks: vec![Task {
                    text: "Fix API".to_string(),
                    is_done: false,
                    due: None,
                }],
            }],
        };
        assert_eq!(sec.name, "work");
        assert_eq!(sec.tasks.len(), 1);
        assert_eq!(sec.subsections.len(), 1);
        assert_eq!(sec.subsections[0].name, "backend");
    }

    #[test]
    fn todolist_default_is_empty() {
        let list = TodoList::default();
        assert!(list.sections.is_empty());
    }

    #[test]
    fn todolist_clone_is_independent() {
        let mut original = TodoList::default();
        original.sections.push(Section {
            name: "main".to_string(),
            tasks: Vec::new(),
            subsections: Vec::new(),
        });
        let cloned = original.clone();
        original.sections.clear();
        assert!(original.sections.is_empty());
        assert_eq!(cloned.sections.len(), 1);
    }
}
