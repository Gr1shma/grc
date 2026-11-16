mod cli;
mod due;
mod parser;
mod task;
mod tui;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let todo_path = match cli.todo_path {
        Some(path) => expand_tilde(path, real_env)?,
        None => resolve_todo_path()?,
    };

    ensure_todo_file_exists(&todo_path).context("Failed to initialize the todo storage file.")?;

    tui::run_tui(&todo_path)
        .context("Site of Grace TUI encountered an unrecoverable runtime error.")?;

    Ok(())
}

fn real_env(name: &str) -> Result<String, env::VarError> {
    env::var(name)
}

fn resolve_todo_path() -> Result<PathBuf> {
    resolve_todo_path_with_env(real_env)
}

fn resolve_todo_path_with_env<F>(get_env: F) -> Result<PathBuf>
where
    F: Fn(&str) -> Result<String, env::VarError> + Copy,
{
    if let Ok(env_path) = get_env("GRC_TODO_PATH") {
        return expand_tilde(PathBuf::from(env_path), get_env);
    }

    Ok(home_dir(get_env)?.join(".todo.md"))
}

fn home_dir<F>(get_env: F) -> Result<PathBuf>
where
    F: Fn(&str) -> Result<String, env::VarError>,
{
    get_env("HOME")
        .map(PathBuf::from)
        .or_else(|_| get_env("USERPROFILE").map(PathBuf::from))
        .map_err(|_| {
            anyhow!("Could not find your system's HOME directory. Please set GRC_TODO_PATH.")
        })
}

/// Expand a leading `~` component using `HOME` / `USERPROFILE`.
fn expand_tilde<F>(path: PathBuf, get_env: F) -> Result<PathBuf>
where
    F: Fn(&str) -> Result<String, env::VarError>,
{
    let Ok(rest) = path.strip_prefix("~") else {
        return Ok(path);
    };
    Ok(home_dir(get_env)?.join(rest))
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
