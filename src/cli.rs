use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "grc",
    about = "A terminal-based todo manager backed by a plain Markdown file.",
    long_about = "Site of Grace - a TUI for managing your tasks in a simple Markdown file.\n\n\
        The todo file is resolved in this order:\n\
        1. PATH argument\n\
        2. GRC_TODO_PATH environment variable\n\
        3. ~/.todo.md"
)]
pub struct Cli {
    /// Path to a specific todo Markdown file
    #[arg(value_name = "PATH")]
    pub todo_path: Option<PathBuf>,
}
