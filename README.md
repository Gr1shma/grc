# grc

A terminal-based todo manager backed by a plain Markdown file.

## Install

```sh
# Clone the repository
git clone https://github.com/Gr1shma/grc.git
cd grc

# Install the binary
cargo install --path .
```

## Usage

```sh
# Open the default file (~/.todo.md)
grc

# Open a specific todo file
grc /path/to/todo.md
```

By default, `grc` reads and writes `~/.todo.md`. You can specify a file path as an argument or override the default path with the `GRC_TODO_PATH` environment variable:

```sh
GRC_TODO_PATH=/path/to/todo.md grc
```

## Keybindings

Press `?` inside the app to open the full help overlay.

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `l` / `Enter` | Focus task panel |
| `h` / `Esc` | Back to sections |
| `Tab` | Switch panel |
| `a` | New heading one level **below** (child of current) |
| `A` | New heading one level **up** (above the current subtree) |
| `o` | New heading at same level, **below** current |
| `O` | New heading at same level, **above** current |
| `i` | Rename / edit heading |
| `t` | Set due date (empty to clear) |
| `x` / `Space` | Toggle done |
| `dd` | Delete |
| `yy` / `p` | Yank / paste |
| `gg` / `G` | Jump top / bottom |
| `q` | Quit |

## Due Date Shortcuts

When setting a due date with `t`, you can type any of:

| Input | Resolves to |
|-------|-------------|
| `YYYY-MM-DD` | Exact date, e.g. `2025-12-31` |
| `today` `tod` `t` | Today |
| `tomorrow` `tmr` `tmw` `tom` | Tomorrow |
| `next week` `nextweek` `nw` | 7 days from today |

Leave the field empty and press `Enter` to clear the due date.

## File Format

Plain Markdown — edit it directly in any editor:

```markdown
# work

- [ ] Fix bug
- [x] Write tests due:2025-06-15

## backend

- [ ] Deploy API

### api

- [ ] Fix endpoint
```
