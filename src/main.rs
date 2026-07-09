mod parser;
mod task;
mod tui;

use anyhow::{Context, Result, anyhow};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let todo_path = determine_todo_path(&args)
        .context("Failed to determine the path for the todo Markdown file.")?;

    ensure_todo_file_exists(&todo_path).context("Failed to initialize the todo storage file.")?;

    tui::run_tui(&todo_path)
        .context("Site of Grace TUI encountered an unrecoverable runtime error.")?;

    Ok(())
}

fn determine_todo_path(args: &[String]) -> Result<PathBuf> {
    if args.len() > 1 {
        Ok(PathBuf::from(&args[1]))
    } else {
        resolve_todo_path()
    }
}

fn resolve_todo_path() -> Result<PathBuf> {
    resolve_todo_path_with_env(|v| env::var(v))
}

fn resolve_todo_path_with_env<F>(get_env: F) -> Result<PathBuf>
where
    F: Fn(&str) -> Result<String, env::VarError>,
{
    if let Ok(env_path) = get_env("GRC_TODO_PATH") {
        return Ok(PathBuf::from(env_path));
    }

    let home = get_env("HOME")
        .map(PathBuf::from)
        .or_else(|_| get_env("USERPROFILE").map(PathBuf::from))
        .map_err(|_| {
            anyhow!("Could not find your system's HOME directory. Please set GRC_TODO_PATH.")
        })?;

    Ok(home.join(".todo.md"))
}

fn ensure_todo_file_exists(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create directory structure: {}", parent.display())
        })?;
    }

    if !path.exists() {
        fs::write(
            path,
            "# main\n- [ ] Welcome to your Site of Grace. Add your first task!\n",
        )
        .with_context(|| {
            format!(
                "Failed to initialize empty markdown file at: {}",
                path.display()
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn resolve_todo_path_uses_env_var_when_set() {
        let mock_env = |var: &str| {
            if var == "GRC_TODO_PATH" {
                Ok("/tmp/custom_grc_test.md".to_string())
            } else {
                Err(env::VarError::NotPresent)
            }
        };
        let result = resolve_todo_path_with_env(mock_env).unwrap();
        assert_eq!(result, PathBuf::from("/tmp/custom_grc_test.md"));
    }

    #[test]
    fn determine_todo_path_uses_first_arg() {
        let args = vec!["grc".to_string(), "my_todo.md".to_string()];
        let path = determine_todo_path(&args).unwrap();
        assert_eq!(path, PathBuf::from("my_todo.md"));
    }

    #[test]
    fn determine_todo_path_falls_back_when_no_arg() {
        let args = vec!["grc".to_string()];
        let path = determine_todo_path(&args);
        assert!(path.is_ok());
    }

    #[test]
    fn resolve_todo_path_falls_back_to_home() {
        let mock_env = |var: &str| {
            if var == "HOME" {
                Ok("/home/user".to_string())
            } else {
                Err(env::VarError::NotPresent)
            }
        };
        let result = resolve_todo_path_with_env(mock_env).unwrap();
        assert_eq!(result, PathBuf::from("/home/user/.todo.md"));
    }

    #[test]
    fn ensure_creates_file_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test_todo.md");
        assert!(!path.exists());
        ensure_todo_file_exists(&path).unwrap();
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# main"));
        assert!(content.contains("Welcome to your Site of Grace"));
    }

    #[test]
    fn ensure_creates_parent_directories() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("deep").join("nested").join("todo.md");
        assert!(!path.exists());
        ensure_todo_file_exists(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn ensure_does_not_overwrite_existing_file() {
        let mut f = NamedTempFile::new().unwrap();
        use std::io::Write;
        write!(f, "# custom\n- [ ] My task\n").unwrap();
        f.flush().unwrap();
        let path = f.path().to_path_buf();

        ensure_todo_file_exists(&path).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# custom"));
        assert!(content.contains("My task"));

        assert!(!content.contains("Welcome to your Site of Grace"));
    }
}
